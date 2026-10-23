package com.ap0dl.anyomi.data

import android.content.Context
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlin.math.ceil

@Serializable
data class HistoryItem(
    val anime: Anime,
    val episode: Episode,
    val positionMs: Long = 0L,
    val durationMs: Long = 0L,
    val updatedAt: Long = 0L,
    val localPath: String? = null,
) {
    val progress: Float
        get() = if (durationMs > 0L) {
            (positionMs.toFloat() / durationMs.toFloat()).coerceIn(0f, 1f)
        } else {
            0f
        }

    /**
     * An episode is only complete when we know its duration and playback has
     * actually reached the credits. A position of one second never qualifies.
     */
    val completed: Boolean
        get() = durationMs > 0L && positionMs >= (durationMs * 0.95).toLong()

    val remainingMs: Long
        get() = if (durationMs > 0L) (durationMs - positionMs).coerceAtLeast(0L) else 0L

    /** Rounded-up minutes, so a 21m 04s remainder reads as 22 minutes. */
    val remainingMinutes: Int
        get() = if (durationMs > 0L) {
            ceil(remainingMs / 60_000.0).toInt().coerceAtLeast(0)
        } else {
            0
        }

    val started: Boolean
        get() = positionMs > 0L
}

class HistoryStore(context: Context) {
    private val prefs = context.getSharedPreferences("anyomi_history", Context.MODE_PRIVATE)
    private val json = Json { ignoreUnknownKeys = true }

    fun load(): List<HistoryItem> {
        val raw = prefs.getString(KEY, null) ?: return emptyList()
        return runCatching { json.decodeFromString<List<HistoryItem>>(raw) }
            .getOrDefault(emptyList())
            .sortedByDescending { it.updatedAt }
    }

    fun save(items: List<HistoryItem>) {
        val trimmed = items.sortedByDescending { it.updatedAt }.take(24)
        prefs.edit().putString(KEY, json.encodeToString(trimmed)).apply()
    }

    fun clear() {
        prefs.edit().remove(KEY).apply()
    }

    private companion object {
        const val KEY = "items"
    }
}