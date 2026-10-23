package com.ap0dl.anyomi

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.ap0dl.anyomi.Details
import com.ap0dl.anyomi.data.Anime
import com.ap0dl.anyomi.data.Api
import com.ap0dl.anyomi.data.DownloadStatus
import com.ap0dl.anyomi.data.DownloadStore
import com.ap0dl.anyomi.data.Episode
import com.ap0dl.anyomi.data.HistoryItem
import com.ap0dl.anyomi.data.HistoryStore
import com.ap0dl.anyomi.data.Loadable
import com.ap0dl.anyomi.data.LocalDownload
import com.ap0dl.anyomi.data.PlayRequest
import com.ap0dl.anyomi.data.ProviderInfo
import com.ap0dl.anyomi.data.ScheduleWeek
import com.ap0dl.anyomi.data.SearchHit
import com.ap0dl.anyomi.data.TrendingChart
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class AppViewModel(application: Application) : AndroidViewModel(application) {
    private val historyStore = HistoryStore(application)
    private val downloadStore = DownloadStore(application)
    private val settings = application.getSharedPreferences("anyomi_settings", Application.MODE_PRIVATE)

    private val _audioPreference = MutableStateFlow(
        settings.getString("audio_preference", "sub").takeIf { it == "dub" } ?: "sub",
    )
    val audioPreference: StateFlow<String> = _audioPreference.asStateFlow()

    private val _providers = MutableStateFlow<Loadable<List<ProviderInfo>>>(Loadable.Loading)
    val providers: StateFlow<Loadable<List<ProviderInfo>>> = _providers.asStateFlow()

    private val _latest = MutableStateFlow<Loadable<List<SearchHit>>>(Loadable.Loading)
    val latest: StateFlow<Loadable<List<SearchHit>>> = _latest.asStateFlow()

    private val _selectedHit = MutableStateFlow<SearchHit?>(null)
    val selectedHit: StateFlow<SearchHit?> = _selectedHit.asStateFlow()

    private val _playRequest = MutableStateFlow<PlayRequest?>(null)
    val playRequest: StateFlow<PlayRequest?> = _playRequest.asStateFlow()

    private val _history = MutableStateFlow(historyStore.load())
    val history: StateFlow<List<HistoryItem>> = _history.asStateFlow()

    private val _activeDownloads = MutableStateFlow<List<DownloadStatus>>(emptyList())
    val activeDownloads: StateFlow<List<DownloadStatus>> = _activeDownloads.asStateFlow()

    private val _localDownloads = MutableStateFlow(downloadStore.scan())
    val localDownloads: StateFlow<List<LocalDownload>> = _localDownloads.asStateFlow()

    private val _downloadNotice = MutableStateFlow<String?>(null)
    val downloadNotice: StateFlow<String?> = _downloadNotice.asStateFlow()

    private val _trending = MutableStateFlow<Loadable<TrendingChart>>(Loadable.Idle)
    val trending: StateFlow<Loadable<TrendingChart>> = _trending.asStateFlow()

    private val _schedule = MutableStateFlow<Loadable<ScheduleWeek>>(Loadable.Idle)
    val schedule: StateFlow<Loadable<ScheduleWeek>> = _schedule.asStateFlow()

    private val detailsCache = mutableMapOf<String, Details>()
    private var downloadPoll: Job? = null

    init {
        refreshProviders()
        refreshLatest()
        refreshDownloads()
    }

    val downloadDirectory: String
        get() = downloadStore.directory.absolutePath

    /**
     * Sub and dub are kept as first-class choices. The preference is used by
     * both playback and downloads, and can be changed from More or the player.
     */
    fun setAudioPreference(value: String) {
        val normalized = if (value.equals("dub", ignoreCase = true)) "dub" else "sub"
        _audioPreference.value = normalized
        settings.edit().putString("audio_preference", normalized).apply()
    }

    fun refreshProviders(force: Boolean = false) {
        if (!force && _providers.value is Loadable.Ready) return
        viewModelScope.launch {
            _providers.value = Loadable.Loading
            _providers.value = try {
                Loadable.Ready(Api.providers())
            } catch (error: Exception) {
                Loadable.Failed(error.message ?: "Could not load sources")
            }
        }
    }

    fun refreshLatest() {
        viewModelScope.launch {
            _latest.value = Loadable.Loading
            _latest.value = try {
                Loadable.Ready(Api.latest("reanime"))
            } catch (error: Exception) {
                Loadable.Failed(error.message ?: "Could not load new episodes")
            }
        }
    }

    fun select(hit: SearchHit) {
        _selectedHit.value = hit
    }

    fun refreshTrending(force: Boolean = false) {
        val current = _trending.value
        if (!force && (current is Loadable.Ready || current is Loadable.Loading)) return
        viewModelScope.launch {
            _trending.value = Loadable.Loading
            _trending.value = try {
                Loadable.Ready(Api.trending())
            } catch (error: Exception) {
                Loadable.Failed(error.message ?: "Could not load the AniTrendz chart")
            }
        }
    }

    fun refreshSchedule(start: Long, end: Long, force: Boolean = false) {
        val current = _schedule.value
        if (!force && (current is Loadable.Ready || current is Loadable.Loading)) return
        viewModelScope.launch {
            _schedule.value = Loadable.Loading
            _schedule.value = try {
                Loadable.Ready(Api.schedule(start, end))
            } catch (error: Exception) {
                Loadable.Failed(error.message ?: "Could not load this week's schedule")
            }
        }
    }

    /** Best-effort title lookup used by Trending/Schedule before opening Details. */
    suspend fun findTitle(title: String): SearchHit? = runCatching {
        Api.search(title, provider = null, limit = 8).firstOrNull()
    }.getOrNull()

    suspend fun details(hit: SearchHit, force: Boolean = false): Details {
        val key = "${hit.provider}|${hit.url}"
        if (force) detailsCache.remove(key)
        detailsCache[key]?.let { return it }
        val details = Api.details(hit)
        detailsCache[key] = details
        return details
    }

    fun play(
        anime: Anime,
        episode: Episode,
        episodes: List<Episode> = emptyList(),
        localPath: String? = null,
    ) {
        _playRequest.value = PlayRequest(anime, episode, episodes, localPath)
        recordStart(anime, episode, localPath)
    }

    fun playLocal(download: LocalDownload) {
        _playRequest.value = PlayRequest(
            anime = download.anime,
            episode = download.episode,
            localPath = download.path,
        )
        recordStart(download.anime, download.episode, download.path)
    }

    fun clearPlayRequest() {
        _playRequest.value = null
    }

    fun recordStart(anime: Anime, episode: Episode, localPath: String? = null) {
        val now = System.currentTimeMillis()
        val existing = _history.value.firstOrNull {
            it.anime.provider == anime.provider && it.anime.url == anime.url &&
                it.episode.number == episode.number
        }
        // Replaying a finished episode should not reopen it at the credits.
        val restarting = existing?.completed == true
        val item = HistoryItem(
            anime = anime,
            episode = episode,
            positionMs = if (restarting) 0L else existing?.positionMs ?: 0L,
            durationMs = existing?.durationMs ?: 0L,
            updatedAt = now,
            localPath = localPath ?: existing?.localPath,
        )
        update(item)
    }

    /**
     * Persists a real playback position. Duration is sticky: ExoPlayer can
     * briefly report 0 while resolving a new stream and that must not erase a
     * known runtime or make an episode look complete.
     */
    fun updateProgress(anime: Anime, episode: Episode, positionMs: Long, durationMs: Long) {
        val existing = _history.value.firstOrNull {
            it.anime.provider == anime.provider && it.anime.url == anime.url &&
                it.episode.number == episode.number
        }
        val safeDuration = when {
            durationMs > 0L -> durationMs
            existing != null && existing.durationMs > 0L -> existing.durationMs
            else -> 0L
        }
        if (positionMs <= 0L && safeDuration <= 0L) return
        val safePosition = if (safeDuration > 0L) {
            positionMs.coerceIn(0L, safeDuration)
        } else {
            positionMs.coerceAtLeast(0L)
        }
        val item = HistoryItem(
            anime = anime,
            episode = episode,
            positionMs = safePosition,
            durationMs = safeDuration,
            updatedAt = System.currentTimeMillis(),
            localPath = existing?.localPath,
        )
        update(item)
    }

    fun removeHistory(item: HistoryItem) {
        val updated = _history.value.filterNot {
            it.anime.provider == item.anime.provider && it.anime.url == item.anime.url &&
                it.episode.number == item.episode.number
        }
        _history.value = updated
        historyStore.save(updated)
    }

    fun startDownload(anime: Anime, episode: Episode) {
        val duplicate = _activeDownloads.value.any {
            it.active && it.animeTitle == anime.title && it.episodeNumber == episode.number
        }
        if (duplicate) {
            _downloadNotice.value = "${episode.label} of ${anime.title} is already downloading"
            return
        }
        viewModelScope.launch {
            try {
                Api.download(
                    anime = anime,
                    episode = episode,
                    directory = downloadStore.directory.absolutePath,
                    audio = audioPreference.value,
                )
                val audioLabel = if (audioPreference.value == "dub") "dub" else "sub"
                _downloadNotice.value = "Queued ${episode.label} of ${anime.title} ($audioLabel)"
                refreshBridgeDownloads()
                startPolling()
            } catch (error: Exception) {
                _downloadNotice.value = error.message ?: "Could not start download"
            }
        }
    }

    fun refreshDownloads() {
        viewModelScope.launch {
            _localDownloads.value = downloadStore.scan()
            refreshBridgeDownloads()
            if (_activeDownloads.value.any { it.active }) startPolling()
        }
    }

    private suspend fun refreshBridgeDownloads() {
        runCatching { Api.downloads() }.onSuccess { tasks ->
            val completed = tasks.filter { it.status == "completed" }
            completed.forEach { persistCompleted(it) }
            _activeDownloads.value = tasks.filter { it.active || it.status == "failed" }
            if (completed.isNotEmpty()) {
                _localDownloads.value = downloadStore.scan()
            }
        }.onFailure {
            _downloadNotice.value = it.message
        }
    }

    private fun startPolling() {
        if (downloadPoll?.isActive == true) return
        downloadPoll = viewModelScope.launch {
            while (true) {
                val tasks = runCatching { Api.downloads() }.getOrNull() ?: break
                val completed = tasks.filter { it.status == "completed" }
                completed.forEach { persistCompleted(it) }
                _activeDownloads.value = tasks.filter { it.active || it.status == "failed" }
                if (completed.isNotEmpty()) {
                    _localDownloads.value = downloadStore.scan()
                }
                if (!tasks.any { it.active }) break
                delay(700)
            }
        }
    }

    private fun persistCompleted(status: DownloadStatus) {
        val path = status.path ?: return
        val anime = Anime(
            id = status.id,
            title = status.animeTitle,
            url = path,
            provider = status.provider,
            cover = status.cover,
        )
        val episode = Episode(
            number = status.episodeNumber,
            title = status.episodeTitle,
            id = status.id,
            url = path,
            audio = "both",
        )
        downloadStore.record(path, anime, episode)
    }

    fun dismissDownload(status: DownloadStatus) {
        viewModelScope.launch {
            runCatching { Api.removeDownload(status.id) }
            _activeDownloads.value = _activeDownloads.value.filterNot { it.id == status.id }
        }
    }

    fun deleteDownload(download: LocalDownload) {
        viewModelScope.launch {
            downloadStore.delete(download)
            _localDownloads.value = downloadStore.scan()
        }
    }

    fun clearDownloadNotice() {
        _downloadNotice.value = null
    }

    fun clearHistory() {
        _history.value = emptyList()
        historyStore.clear()
    }

    private fun update(item: HistoryItem, persist: Boolean = true) {
        val updated = buildList {
            add(item)
            addAll(_history.value.filterNot {
                it.anime.provider == item.anime.provider && it.anime.url == item.anime.url &&
                    it.episode.number == item.episode.number
            })
        }.sortedByDescending { it.updatedAt }
        _history.value = updated
        if (persist) historyStore.save(updated)
    }
}