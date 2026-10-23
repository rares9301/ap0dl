package com.ap0dl.anyomi.data

import com.ap0dl.anyomi.Details
import com.ap0dl.anyomi.MobileEngine
import com.ap0dl.anyomi.Playback
import com.ap0dl.anyomi.data.Chapter
import com.ap0dl.anyomi.data.ScheduleWeek
import com.ap0dl.anyomi.data.TrendingChart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Coroutine-facing engine API. All native calls happen on the IO dispatcher. */
object Api {
    suspend fun providers(): List<ProviderInfo> = withContext(Dispatchers.IO) {
        MobileEngine.providers()
    }

    suspend fun latest(provider: String? = null): List<SearchHit> = withContext(Dispatchers.IO) {
        MobileEngine.latest(provider)
    }

    suspend fun search(query: String, provider: String? = null, limit: Int = 30): List<SearchHit> =
        withContext(Dispatchers.IO) {
            MobileEngine.search(query, provider, limit)
        }

    suspend fun details(hit: SearchHit): Details = withContext(Dispatchers.IO) {
        MobileEngine.details(hit.url, hit.provider)
    }

    suspend fun streams(
        anime: Anime,
        episode: Episode,
        quality: String? = null,
        audio: String = "sub",
    ): Playback = withContext(Dispatchers.IO) {
        MobileEngine.streams(anime, episode, quality, audio)
    }

    suspend fun download(
        anime: Anime,
        episode: Episode,
        directory: String,
        quality: String? = null,
        audio: String = "sub",
    ): String = withContext(Dispatchers.IO) {
        MobileEngine.download(anime, episode, directory, quality, audio)
    }

    suspend fun downloads(): List<DownloadStatus> = withContext(Dispatchers.IO) {
        MobileEngine.downloads()
    }

    suspend fun removeDownload(taskId: String) = withContext(Dispatchers.IO) {
        MobileEngine.removeDownload(taskId)
    }

    suspend fun trending(): TrendingChart = withContext(Dispatchers.IO) {
        MobileEngine.trending()
    }

    suspend fun schedule(start: Long, end: Long): ScheduleWeek = withContext(Dispatchers.IO) {
        MobileEngine.schedule(start, end)
    }

    suspend fun skipTimes(anime: Anime, episode: Episode, durationMs: Long): List<Chapter> =
        withContext(Dispatchers.IO) {
            MobileEngine.skipTimes(anime, episode, durationMs)
        }
}