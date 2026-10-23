package com.ap0dl.anyomi.data

import android.content.Context
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File

@Serializable
data class DownloadRecord(
    val anime: Anime,
    val episode: Episode,
    val completedAt: Long = 0L,
)

data class LocalDownload(
    val file: File,
    val anime: Anime,
    val episode: Episode,
    val size: Long,
    val completedAt: Long,
) {
    val path: String get() = file.absolutePath
    val title: String get() = anime.title
    val episodeLabel: String get() = episode.label
    val provider: String get() = anime.provider
    val cover: String? get() = anime.cover
}

class DownloadStore(context: Context) {
    val directory: File = File(context.filesDir, "downloads").apply { mkdirs() }
    private val json = Json { ignoreUnknownKeys = true }
    private val videoExtensions = setOf("mp4", "mkv", "webm", "ts", "m4v", "mov", "avi")

    fun scan(): List<LocalDownload> {
        return directory.walkTopDown()
            .filter { it.isFile && it.extension.lowercase() in videoExtensions }
            .mapNotNull { file ->
                val record = readRecord(file)
                val anime = record?.anime ?: Anime(
                    id = file.name,
                    title = file.nameWithoutExtension,
                    url = file.toURI().toString(),
                    provider = "local",
                )
                val episode = record?.episode ?: Episode(
                    number = episodeNumber(file.nameWithoutExtension),
                    title = file.nameWithoutExtension,
                    id = file.absolutePath,
                    url = file.toURI().toString(),
                    audio = "both",
                )
                LocalDownload(
                    file = file,
                    anime = anime,
                    episode = episode,
                    size = file.length(),
                    completedAt = record?.completedAt ?: file.lastModified(),
                )
            }
            .sortedByDescending { it.completedAt }
            .toList()
    }

    fun record(path: String, anime: Anime, episode: Episode) {
        val file = File(path)
        if (!file.exists()) return
        val record = DownloadRecord(
            anime = anime,
            episode = episode,
            completedAt = System.currentTimeMillis(),
        )
        runCatching {
            sidecar(file).writeText(json.encodeToString(record))
        }
    }

    fun delete(download: LocalDownload) {
        runCatching { download.file.delete() }
        runCatching { sidecar(download.file).delete() }
        runCatching {
            download.file.parentFile?.listFiles()
                ?.filter {
                    it.name.startsWith(download.file.nameWithoutExtension) &&
                        (it.name.endsWith(".srt") || it.name.endsWith(".vtt") || it.name.endsWith(".ass"))
                }
                ?.forEach { it.delete() }
        }
    }

    private fun readRecord(file: File): DownloadRecord? {
        val sidecar = sidecar(file)
        if (!sidecar.exists()) return null
        return runCatching {
            json.decodeFromString<DownloadRecord>(sidecar.readText())
        }.getOrNull()
    }

    private fun sidecar(file: File): File = File(file.parentFile, "${file.name}.anyomi.json")

    private fun episodeNumber(name: String): Float {
        val match = Regex("""[Ee](\d+(?:\.\d+)?)""").find(name) ?: return 0f
        return match.groupValues[1].toFloatOrNull() ?: 0f
    }
}