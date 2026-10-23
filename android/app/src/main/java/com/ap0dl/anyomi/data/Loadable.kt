package com.ap0dl.anyomi.data

sealed interface Loadable<out T> {
    data object Idle : Loadable<Nothing>
    data object Loading : Loadable<Nothing>
    data class Ready<T>(val value: T) : Loadable<T>
    data class Failed(val message: String) : Loadable<Nothing>
}

data class PlayRequest(
    val anime: Anime,
    val episode: Episode,
    val episodes: List<Episode> = emptyList(),
    val localPath: String? = null,
)

fun Loadable<*>.errorMessage(): String? = (this as? Loadable.Failed)?.message