package com.ap0dl.anyomi.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class SearchHit(
    val id: String = "",
    val title: String = "",
    val url: String = "",
    val provider: String = "",
    @SerialName("provider_name") val providerName: String = "",
    val cover: String? = null,
    val year: Int? = null,
    val format: String? = null,
    val episodes: Int? = null,
)

@Serializable
data class Anime(
    val id: String = "",
    val title: String = "",
    val url: String = "",
    val provider: String = "",
    val cover: String? = null,
    val description: String? = null,
    val genres: List<String> = emptyList(),
    val year: Int? = null,
    val format: String? = null,
    val status: String? = null,
    @SerialName("total_episodes") val totalEpisodes: Int? = null,
    @SerialName("anilist_id") val anilistId: Long? = null,
    @SerialName("mal_id") val malId: Long? = null,
)

@Serializable
data class Episode(
    val number: Float = 0f,
    val title: String? = null,
    val id: String = "",
    val url: String = "",
    val filler: Boolean = false,
    val recap: Boolean = false,
    val aired: String? = null,
    val audio: String = "sub",
) {
    val label: String
        get() = if (number % 1f == 0f) {
            "Episode ${number.toInt()}"
        } else {
            "Episode $number"
        }
}

@Serializable
data class ProviderInfo(
    val id: String = "",
    val name: String = "",
    @SerialName("base_url") val baseUrl: String = "",
    val status: String = "experimental",
    val notes: String = "",
)

@Serializable
data class Subtitle(
    val url: String,
    val lang: String = "",
    val format: String? = null,
    val default: Boolean = false,
)

@Serializable
data class Chapter(
    val start: Double = 0.0,
    val end: Double = 0.0,
    val title: String = "",
)

fun String.audioLabel(): String = when (lowercase()) {
    "sub" -> "Sub"
    "dub" -> "Dub"
    "both" -> "Sub & Dub"
    "raw" -> "Raw"
    else -> replaceFirstChar { it.uppercase() }
}

fun Episode.displayTitle(): String {
    val clean = title?.trim().orEmpty()
    return if (clean.isEmpty() || clean.contains("episode", ignoreCase = true)) {
        label
    } else {
        "$label · $clean"
    }
}

fun SearchHit.toAnime(): Anime = Anime(
    id = id,
    title = title,
    url = url,
    provider = provider,
    cover = cover,
    year = year,
    format = format,
    totalEpisodes = episodes,
)

@Serializable
data class DownloadStatus(
    val id: String = "",
    @SerialName("anime_title") val animeTitle: String = "",
    @SerialName("episode_number") val episodeNumber: Float = 0f,
    @SerialName("episode_title") val episodeTitle: String? = null,
    val provider: String = "",
    val cover: String? = null,
    val status: String = "queued",
    @SerialName("downloaded_bytes") val downloadedBytes: Long = 0L,
    @SerialName("total_bytes") val totalBytes: Long? = null,
    @SerialName("segments_done") val segmentsDone: Long? = null,
    @SerialName("segments_total") val segmentsTotal: Long? = null,
    val path: String? = null,
    val subtitles: List<String> = emptyList(),
    val error: String? = null,
    val audio: String? = null,
    @SerialName("audio_fallback") val audioFallback: Boolean = false,
) {
    val active: Boolean
        get() = status == "queued" || status == "resolving" || status == "downloading"

    val progress: Float?
        get() = when {
            totalBytes != null && totalBytes > 0L ->
                (downloadedBytes.toFloat() / totalBytes.toFloat()).coerceIn(0f, 1f)
            segmentsTotal != null && segmentsTotal > 0L ->
                ((segmentsDone ?: 0L).toFloat() / segmentsTotal.toFloat()).coerceIn(0f, 1f)
            else -> null
        }
}

@Serializable
data class TaskId(
    @SerialName("task_id") val taskId: String = "",
)

@Serializable
data class TrendingChart(
    val title: String = "Top Anime",
    val slug: String = "top-anime",
    val week: String = "",
    val season: String = "",
    val date: String = "",
    val votes: Long = 0L,
    val entries: List<TrendingEntry> = emptyList(),
    @SerialName("fetched_from") val fetchedFrom: String = "",
)

@Serializable
data class TrendingEntry(
    val rank: Int = 0,
    val title: String = "",
    @SerialName("alt_title") val altTitle: String? = null,
    val cover: String? = null,
    val studio: String? = null,
    val tag: String? = null,
    val votes: Long? = null,
    val previous: Int? = null,
    val peak: Int? = null,
    @SerialName("weeks_on_chart") val weeksOnChart: Int? = null,
    val uid: String? = null,
) {
    /** Positive is upward movement; null means this is a new entry. */
    val movement: Int?
        get() = previous?.takeIf { it > 0 }?.let { it - rank }

    val isNew: Boolean
        get() = previous == null || previous == 0
}

@Serializable
data class ScheduleWeek(
    val start: Long = 0L,
    val end: Long = 0L,
    val entries: List<ScheduleEntry> = emptyList(),
)

@Serializable
data class ScheduleEntry(
    val id: Long = 0L,
    val title: String = "",
    @SerialName("native_title") val nativeTitle: String? = null,
    val cover: String? = null,
    val episode: Int = 0,
    @SerialName("airing_at") val airingAt: Long = 0L,
    val format: String? = null,
    val status: String? = null,
    @SerialName("site_url") val siteUrl: String? = null,
    val genres: List<String> = emptyList(),
)
