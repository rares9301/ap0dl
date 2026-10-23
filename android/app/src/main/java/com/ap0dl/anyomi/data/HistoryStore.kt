package com.ap0dl.anyomi.data

import android.content.Context
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

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