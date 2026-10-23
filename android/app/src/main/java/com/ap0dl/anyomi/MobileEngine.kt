package com.ap0dl.anyomi

import com.ap0dl.anyomi.data.Anime
import com.ap0dl.anyomi.data.Episode
import com.ap0dl.anyomi.data.ProviderInfo
import com.ap0dl.anyomi.data.SearchHit
import com.ap0dl.anyomi.data.TaskId
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * Blocking JNI facade over ap0dl-core.
 *
 * Call these methods from Dispatchers.IO. Keeping the blocking boundary in
 * exactly one file makes the rest of the UI ordinary coroutine/Compose code.
 */
object MobileEngine {
    init {
        System.loadLibrary("ap0dl_mobile")
    }

    private external fun nativeProviders(): String
    private external fun nativeLatest(provider: String?): String
    private external fun nativeSearch(query: String, provider: String?, limit: Int): String
    private external fun nativeDetails(url: String, provider: String?): String
    private external fun nativeStreams(
        animeJson: String,
        episodeJson: String,
        quality: String?,
        audio: String?,
    ): String
    private external fun nativeDownload(
        animeJson: String,
        episodeJson: String,
        directory: String,
        quality: String?,
        audio: String?,
    ): String
    private external fun nativeDownloads(): String
    private external fun nativeDownloadRemove(taskId: String): String

    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    fun providers(): List<ProviderInfo> = decode(nativeProviders())

    fun latest(provider: String? = null): List<SearchHit> = decode(nativeLatest(provider))

    fun search(query: String, provider: String? = null, limit: Int = 30): List<SearchHit> =
        decode(nativeSearch(query, provider, limit))

    fun details(url: String, provider: String?): Details = decode(nativeDetails(url, provider))

    fun streams(
        anime: Anime,
        episode: Episode,
        quality: String? = null,
        audio: String = "sub",
    ): Playback = decode(
        nativeStreams(
            json.encodeToString(anime),
            json.encodeToString(episode),
            quality,
            audio,
        ),
    )

    fun download(
        anime: Anime,
        episode: Episode,
        directory: String,
        quality: String? = null,
        audio: String = "sub",
    ): String = decode<TaskId>(
        nativeDownload(
            json.encodeToString(anime),
            json.encodeToString(episode),
            directory,
            quality,
            audio,
        ),
    ).taskId

    fun downloads(): List<com.ap0dl.anyomi.data.DownloadStatus> = decode(nativeDownloads())

    fun removeDownload(taskId: String) {
        decode<Boolean>(nativeDownloadRemove(taskId))
    }

    private inline fun <reified T> decode(raw: String): T {
        if (raw.isBlank()) throw EngineException("The engine returned an empty response")
        val envelope = try {
            json.decodeFromString<Envelope<T>>(raw)
        } catch (error: Exception) {
            throw EngineException("Could not read engine response: ${error.message}")
        }
        if (!envelope.ok) throw EngineException(envelope.error ?: "Unknown engine error")
        return envelope.data ?: throw EngineException("The engine returned no data")
    }
}

@Serializable
data class Envelope<T>(
    val ok: Boolean = true,
    val data: T? = null,
    val error: String? = null,
)

@Serializable
data class Details(
    val anime: Anime,
    val episodes: List<Episode> = emptyList(),
    val provider: ProviderInfo,
    @SerialName("episode_error") val episodeError: String? = null,
)

@Serializable
data class Playback(
    val url: String,
    val kind: String,
    val quality: Int? = null,
    val qualities: List<Int> = emptyList(),
    val title: String? = null,
    val audio: String = "sub",
    val audios: List<String> = emptyList(),
    @SerialName("requested_audio") val requestedAudio: String = "sub",
    @SerialName("audio_fallback") val audioFallback: Boolean = false,
    val headers: Map<String, String> = emptyMap(),
    val subtitles: List<com.ap0dl.anyomi.data.Subtitle> = emptyList(),
    val chapters: List<com.ap0dl.anyomi.data.Chapter> = emptyList(),
    val container: String? = null,
    val proxied: Boolean = false,
    @SerialName("mime_type") val mimeType: String? = null,
)

class EngineException(message: String) : Exception(message)