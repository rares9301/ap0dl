package com.ap0dl.anyomi.ui.screens

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.content.pm.ActivityInfo
import android.net.Uri
import android.view.View
import android.view.ViewGroup
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.HighQuality
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.SkipNext
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Tracks
import androidx.media3.common.util.Util
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.dash.DashMediaSource
import androidx.media3.exoplayer.hls.HlsMediaSource
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.exoplayer.source.MergingMediaSource
import androidx.media3.exoplayer.source.ProgressiveMediaSource
import androidx.media3.exoplayer.source.SingleSampleMediaSource
import androidx.media3.exoplayer.text.TextRenderer
import androidx.media3.exoplayer.trackselection.DefaultTrackSelector
import androidx.media3.extractor.DefaultExtractorsFactory
import androidx.media3.ui.AspectRatioFrameLayout
import androidx.media3.ui.PlayerView
import com.ap0dl.anyomi.AppViewModel
import com.ap0dl.anyomi.Playback
import com.ap0dl.anyomi.data.Anime
import com.ap0dl.anyomi.data.Api
import com.ap0dl.anyomi.data.Episode
import com.ap0dl.anyomi.data.PlayRequest
import com.ap0dl.anyomi.data.Subtitle
import com.ap0dl.anyomi.data.displayTitle
import com.ap0dl.anyomi.ui.components.CoverImage
import com.ap0dl.anyomi.ui.components.ErrorPanel
import com.ap0dl.anyomi.ui.components.GhostAction
import com.ap0dl.anyomi.ui.components.PrimaryAction
import com.ap0dl.anyomi.ui.components.SourcePill
import com.ap0dl.anyomi.ui.theme.Amber
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.Paper
import kotlinx.coroutines.delay
import java.io.File

private sealed interface StreamState {
    data object Loading : StreamState
    data class Ready(val playback: Playback) : StreamState
    data class Failed(val message: String) : StreamState
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PlayerScreen(
    app: AppViewModel,
    request: PlayRequest,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    val activity = context.findActivity()
    val view = LocalView.current
    val anime = request.anime
    val episode = request.episode
    var qualitySpec by remember(anime.url) { mutableStateOf<String?>(null) }
    var audioSpec by remember(anime.url) { mutableStateOf(app.audioPreference.value) }
    var streamState by remember(anime.url) { mutableStateOf<StreamState>(StreamState.Loading) }
    var playerError by remember(anime.url) { mutableStateOf<String?>(null) }
    var refreshAttempt by remember(anime.url) { mutableIntStateOf(0) }
    var controlsVisible by remember { mutableStateOf(true) }
    var ended by remember { mutableStateOf(false) }
    var showQuality by remember { mutableStateOf(false) }
    var showAudio by remember { mutableStateOf(false) }
    var activeAudioTrack by remember(anime.url) { mutableStateOf<String?>(null) }
    var showSubtitleInfo by remember { mutableStateOf(false) }
    var queue by remember(anime.url) { mutableStateOf(request.episodes.sortedBy { it.number }) }
    val history by app.history.collectAsStateWithLifecycle()
    val currentEpisode by rememberUpdatedState(episode)

    val trackSelector = remember(anime.url) { DefaultTrackSelector(context) }
    val player = remember(anime.url) {
        ExoPlayer.Builder(context)
            .setTrackSelector(trackSelector)
            .setSeekBackIncrementMs(10_000)
            .setSeekForwardIncrementMs(10_000)
            .build()
            .apply {
                setAudioAttributes(
                    AudioAttributes.Builder()
                        .setContentType(C.AUDIO_CONTENT_TYPE_MOVIE)
                        .setUsage(C.USAGE_MEDIA)
                        .build(),
                    true,
                )
                setHandleAudioBecomingNoisy(true)
                // Providers commonly expose SubRip/WebVTT side-loaded tracks.
                // Media3 keeps raw-text decoding behind this flag.
                for (index in 0 until rendererCount) {
                    (getRenderer(index) as? TextRenderer)
                        ?.experimentalSetLegacyDecodingEnabled(true)
                }
            }
    }

    // Dual-audio files expose both tracks inside one stream. Tell ExoPlayer
// which one to prefer so the Sub/Dub choice still applies there.
    LaunchedEffect(trackSelector, audioSpec) {
        val language = if (audioSpec == "dub") "en" else "ja"
        trackSelector.parameters = trackSelector.parameters.buildUpon()
            .setPreferredAudioLanguage(language)
            .build()
    }

    // Restore a saved position for this episode, but never jump straight back
    // to the credits.
    val resumePosition = remember(anime.url, episode.id) {
        history.firstOrNull {
            it.anime.url == anime.url && it.episode.number == episode.number
        }?.takeIf { it.progress < 0.96f }?.positionMs ?: 0L
    }
    var loadedEpisodeId by remember(anime.url) { mutableStateOf<String?>(null) }

    DisposableEffect(player) {
        val listener = object : Player.Listener {
            override fun onPlaybackStateChanged(playbackState: Int) {
                if (playbackState == Player.STATE_ENDED) {
                    ended = true
                    controlsVisible = true
                    val duration = player.duration.coerceAtLeast(0L)
                    app.updateProgress(anime, currentEpisode, duration, duration)
                }
            }

            override fun onPlayerError(error: PlaybackException) {
                val detail = error.message.orEmpty()
                playerError = if (
                    detail.contains("unexpected end of stream", ignoreCase = true) ||
                    error.errorCode == PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED
                ) {
                    "The source stopped sending video. This is usually a provider quota or rate limit. " +
                        "Try another episode or source."
                } else {
                    "${error.errorCodeName}: ${detail.ifBlank { "playback failed" }}"
                }
                controlsVisible = true
            }

            override fun onIsPlayingChanged(isPlaying: Boolean) {
                if (isPlaying) ended = false
            }

            override fun onTracksChanged(tracks: Tracks) {
                val group = tracks.groups.firstOrNull {
                    it.type == C.TRACK_TYPE_AUDIO && it.isSelected
                }
                val index = group?.let { candidate ->
                    (0 until candidate.length).firstOrNull { candidate.isTrackSelected(it) }
                }
                val format = if (group != null && index != null) {
                    group.getTrackFormat(index)
                } else {
                    null
                }
                activeAudioTrack = format?.let { track ->
                    val language = when (track.language?.lowercase()) {
                        "en", "eng" -> "English"
                        "ja", "jpn" -> "Japanese"
                        "es", "spa" -> "Spanish"
                        "fr", "fra", "fre" -> "French"
                        "de", "deu", "ger" -> "German"
                        "pt", "por" -> "Portuguese"
                        else -> track.label
                    }
                    language ?: track.label ?: track.language ?: "Unknown"
                }
            }
        }
        player.addListener(listener)
        onDispose {
            val duration = player.duration.coerceAtLeast(0L)
            val position = player.currentPosition.coerceAtLeast(0L)
            if (position > 0L) {
                app.updateProgress(anime, currentEpisode, position, duration)
            }
            player.removeListener(listener)
            player.release()
        }
    }

    LaunchedEffect(anime.url) {
        if (queue.isEmpty()) {
            runCatching {
                app.details(
                    com.ap0dl.anyomi.data.SearchHit(
                        id = anime.id,
                        title = anime.title,
                        url = anime.url,
                        provider = anime.provider,
                        providerName = anime.provider,
                        cover = anime.cover,
                        year = anime.year,
                        format = anime.format,
                        episodes = anime.totalEpisodes,
                    ),
                )
            }.onSuccess { queue = it.episodes.sortedBy { ep -> ep.number } }
        }
    }

    LaunchedEffect(anime.url, episode.id, qualitySpec, audioSpec, refreshAttempt, request.localPath) {
        streamState = StreamState.Loading
        activeAudioTrack = null
        playerError = null
        ended = false
        val localPath = request.localPath
        if (localPath != null) {
            val file = File(localPath)
            if (!file.exists()) {
                streamState = StreamState.Failed("The downloaded file no longer exists")
            } else {
                val base = file.nameWithoutExtension
                val subtitles = file.parentFile?.listFiles()
                    ?.filter {
                        it.isFile && it.name.startsWith(base) &&
                            (it.extension.equals("srt", true) || it.extension.equals("vtt", true))
                    }
                    ?.sortedBy { it.name }
                    ?.mapIndexed { index, subtitle ->
                        com.ap0dl.anyomi.data.Subtitle(
                            url = Uri.fromFile(subtitle).toString(),
                            lang = "Downloaded ${index + 1}",
                            format = subtitle.extension,
                            default = index == 0,
                        )
                    }
                    .orEmpty()
                streamState = StreamState.Ready(
                    Playback(
                        url = Uri.fromFile(file).toString(),
                        kind = "direct",
                        title = file.name,
                        audio = episode.audio,
                        subtitles = subtitles,
                        mimeType = localMime(file.extension),
                    ),
                )
            }
            return@LaunchedEffect
        }
        streamState = try {
            StreamState.Ready(Api.streams(anime, episode, qualitySpec, audioSpec))
        } catch (error: Exception) {
            StreamState.Failed(error.message ?: "Could not resolve a stream")
        }
    }

    // Tell the viewer plainly when a source cannot honour the chosen audio
    // instead of pretending the dub stream exists.
    val readyPlayback = (streamState as? StreamState.Ready)?.playback
    LaunchedEffect(readyPlayback?.audioFallback, readyPlayback?.requestedAudio, readyPlayback?.audio) {
        val playback = readyPlayback ?: return@LaunchedEffect
        if (!playback.audioFallback) return@LaunchedEffect
        val wanted = playback.requestedAudio.audioName()
        val got = playback.audio.audioName()
        Toast.makeText(
            context,
            "$wanted audio is not offered by this source · playing $got",
            Toast.LENGTH_LONG,
        ).show()
    }

    LaunchedEffect(streamState, episode.id) {
        val ready = streamState as? StreamState.Ready ?: return@LaunchedEffect
        val startPosition = if (loadedEpisodeId == episode.id) {
            player.currentPosition.coerceAtLeast(0L)
        } else {
            resumePosition
        }
        loadedEpisodeId = episode.id
        val source = buildMediaSource(context, ready.playback)
        player.setMediaSource(source, startPosition)
        player.prepare()
        player.playWhenReady = true
        playerError = null
    }

    LaunchedEffect(player, episode.id) {
        while (true) {
            delay(5_000)
            if (player.isPlaying) {
                val duration = player.duration.coerceAtLeast(0L)
                val position = player.currentPosition.coerceAtLeast(0L)
                if (duration > 0L && position > 0L) {
                    app.updateProgress(anime, episode, position, duration)
                }
            }
        }
    }

    DisposableEffect(Unit) {
        view.keepScreenOn = true
        val window = activity?.window
        val controller = window?.let { WindowCompat.getInsetsController(it, it.decorView) }
        controller?.systemBarsBehavior =
            WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        controller?.hide(WindowInsetsCompat.Type.systemBars())
        val previousOrientation = activity?.requestedOrientation
        activity?.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
        onDispose {
            view.keepScreenOn = false
            controller?.show(WindowInsetsCompat.Type.systemBars())
            if (activity != null && previousOrientation != null) {
                activity.requestedOrientation = previousOrientation
            }
        }
    }

    fun back() {
        val duration = player.duration.coerceAtLeast(0L)
        val position = player.currentPosition.coerceAtLeast(0L)
        if (position > 0L) app.updateProgress(anime, episode, position, duration)
        onBack()
    }

    BackHandler {
        back()
    }

    val nextEpisode = remember(queue, episode.id) {
        val index = queue.indexOfFirst { it.id == episode.id || it.number == episode.number }
        if (index >= 0) queue.getOrNull(index + 1) else null
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        AndroidView(
            factory = { ctx ->
                PlayerView(ctx).apply {
                    layoutParams = ViewGroup.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT,
                        ViewGroup.LayoutParams.MATCH_PARENT,
                    )
                    this.player = player
                    useController = true
                    controllerAutoShow = true
                    controllerShowTimeoutMs = 3_500
                    setShowBuffering(PlayerView.SHOW_BUFFERING_WHEN_PLAYING)
                    setKeepContentOnPlayerReset(true)
                    resizeMode = AspectRatioFrameLayout.RESIZE_MODE_FIT
                    setBackgroundColor(android.graphics.Color.BLACK)
                    setControllerVisibilityListener(
                        PlayerView.ControllerVisibilityListener { visibility ->
                            controlsVisible = visibility == View.VISIBLE
                        },
                    )
                }
            },
            update = { playerView ->
                playerView.player = player
                if (ended) playerView.showController()
            },
            modifier = Modifier.fillMaxSize(),
        )

        val state = streamState
        if (state is StreamState.Loading) {
            StreamLoadingOverlay(anime, episode)
        }
        if (state is StreamState.Failed) {
            StreamFailureOverlay(
                message = state.message,
                onRetry = { refreshAttempt++ },
                onBack = { back() },
            )
        }
        if (playerError != null) {
            StreamFailureOverlay(
                message = playerError!!,
                onRetry = {
                    playerError = null
                    loadedEpisodeId = null
                    refreshAttempt++
                },
                onBack = { back() },
                retryLabel = "Resolve again",
            )
        }

        AnimatedVisibility(
            visible = controlsVisible && state is StreamState.Ready && playerError == null,
            enter = fadeIn(),
            exit = fadeOut(),
        ) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .background(
                        Brush.verticalGradient(
                            listOf(Color(0xCC000000), Color(0x55000000), Color.Transparent),
                        ),
                    )
                    .statusBarsPadding()
                    .padding(horizontal = 10.dp, vertical = 8.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    IconButton(onClick = { back() }) {
                        Icon(
                            Icons.AutoMirrored.Rounded.ArrowBack,
                            contentDescription = "Back",
                            tint = Color.White,
                        )
                    }
                    Column(Modifier.weight(1f)) {
                        Text(
                            anime.title,
                            color = Color.White,
                            style = MaterialTheme.typography.titleMedium,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        Text(
                            episode.displayTitle(),
                            color = Color.White.copy(alpha = 0.66f),
                            style = MaterialTheme.typography.bodySmall,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    val playback = (state as? StreamState.Ready)?.playback
                    if (playback != null && request.localPath == null) {
                        Surface(
                            modifier = Modifier
                                .clip(RoundedCornerShape(50))
                                .clickable { showAudio = true },
                            color = Color.White.copy(alpha = 0.11f),
                            shape = RoundedCornerShape(50),
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(5.dp),
                            ) {
                                Icon(
                                    Icons.Rounded.Translate,
                                    contentDescription = null,
                                    tint = if (playback.audioFallback) Amber else Ice,
                                    modifier = Modifier.size(16.dp),
                                )
                                Text(
                                    audioChipLabel(
                                        playback.requestedAudio,
                                        playback.audio,
                                        playback.audioFallback,
                                    ),
                                    color = if (playback.audioFallback) Amber else Ice,
                                    style = MaterialTheme.typography.labelMedium,
                                )
                            }
                        }
                        Surface(
                            modifier = Modifier
                                .clip(RoundedCornerShape(50))
                                .clickable { showQuality = true },
                            color = Color.White.copy(alpha = 0.11f),
                            shape = RoundedCornerShape(50),
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(5.dp),
                            ) {
                                Icon(
                                    Icons.Rounded.HighQuality,
                                    contentDescription = null,
                                    tint = Ice,
                                    modifier = Modifier.size(16.dp),
                                )
                                Text(
                                    playback.quality?.let { "${it}p" } ?: "AUTO",
                                    color = Ice,
                                    style = MaterialTheme.typography.labelMedium,
                                )
                            }
                        }
                        if (playback.subtitles.isNotEmpty()) {
                            Surface(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(50))
                                    .clickable { showSubtitleInfo = true },
                                color = Color.White.copy(alpha = 0.11f),
                                shape = RoundedCornerShape(50),
                            ) {
                                Text(
                                    "CC",
                                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
                                    color = Color.White,
                                    style = MaterialTheme.typography.labelMedium,
                                )
                            }
                        }
                    }
                }
            }
        }

        if (ended) {
            EndedOverlay(
                anime = anime,
                episode = episode,
                nextEpisode = nextEpisode,
                onReplay = {
                    ended = false
                    player.seekTo(0)
                    player.play()
                },
                onNext = nextEpisode?.let { next ->
                    {
                        ended = false
                        app.play(anime, next, queue)
                    }
                },
                onBack = { back() },
            )
        }

        if (showQuality) {
            val playback = (streamState as? StreamState.Ready)?.playback
            QualitySheet(
                qualities = playback?.qualities.orEmpty(),
                currentSpec = qualitySpec,
                currentQuality = playback?.quality,
                onDismiss = { showQuality = false },
                onSelect = {
                    qualitySpec = it
                    showQuality = false
                },
                onAuto = {
                    qualitySpec = null
                    showQuality = false
                },
            )
        }

        if (showAudio) {
            val playback = (streamState as? StreamState.Ready)?.playback
            AudioSheet(
                audios = playback?.audios.orEmpty(),
                currentSpec = audioSpec,
                currentAudio = playback?.audio.orEmpty(),
                activeTrack = activeAudioTrack,
                fallback = playback?.audioFallback == true,
                onDismiss = { showAudio = false },
                onSelect = {
                    app.setAudioPreference(it)
                    audioSpec = it
                    showAudio = false
                },
            )
        }

        if (showSubtitleInfo) {
            val subs = (streamState as? StreamState.Ready)?.playback?.subtitles.orEmpty()
            ModalBottomSheet(
                onDismissRequest = { showSubtitleInfo = false },
                containerColor = Night,
            ) {
                Column(Modifier.padding(horizontal = 20.dp).padding(bottom = 28.dp)) {
                    Text(
                        "Subtitles",
                        color = Paper,
                        style = MaterialTheme.typography.headlineSmall,
                    )
                    Spacer(Modifier.height(6.dp))
                    Text(
                        "Track selection is also available from the player's settings button.",
                        color = Muted,
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Spacer(Modifier.height(14.dp))
                    subs.take(12).forEach { subtitle ->
                        Row(
                            Modifier.fillMaxWidth().padding(vertical = 9.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(9.dp),
                        ) {
                            Box(
                                Modifier.size(7.dp).clip(CircleShape).background(
                                    if (subtitle.default || subtitle.lang.contains("english", true)) Mint else Ice,
                                ),
                            )
                            Text(
                                subtitle.lang.ifBlank { "Unknown" },
                                color = Paper,
                                style = MaterialTheme.typography.bodyMedium,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun StreamLoadingOverlay(anime: Anime, episode: Episode) {
    Box(Modifier.fillMaxSize()) {
        CoverImage(
            url = anime.cover,
            title = anime.title,
            modifier = Modifier.fillMaxSize(),
        )
        Box(
            Modifier
                .fillMaxSize()
                .background(
                    Brush.verticalGradient(
                        listOf(Color(0xE60B0E14), Color(0xF70B0E14), Color(0xFF0B0E14)),
                    ),
                ),
        )
        Column(
            modifier = Modifier.align(Alignment.Center).padding(28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            CircularProgressIndicator(color = Ice, strokeWidth = 3.dp)
            Text(
                "Resolving stream",
                color = Paper,
                style = MaterialTheme.typography.headlineSmall,
            )
            Text(
                "${anime.title} · ${episode.label}",
                color = Muted,
                style = MaterialTheme.typography.bodyMedium,
            )
            SourcePill(providerId = anime.provider)
        }
    }
}

@Composable
private fun StreamFailureOverlay(
    message: String,
    onRetry: () -> Unit,
    onBack: () -> Unit,
    retryLabel: String = "Retry",
) {
    Box(
        Modifier.fillMaxSize().background(Color(0xE60B0E14)),
        contentAlignment = Alignment.Center,
    ) {
        Column(Modifier.padding(28.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            ErrorPanel(message = message, onRetry = onRetry)
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                GhostAction(text = "Go back", onClick = onBack)
                Surface(
                    color = Color.White.copy(alpha = 0.06f),
                    shape = RoundedCornerShape(14.dp),
                ) {
                    Row(
                        Modifier.clickable(onClick = onRetry).padding(horizontal = 16.dp, vertical = 14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Icon(
                            Icons.Rounded.Refresh,
                            contentDescription = null,
                            tint = Ice,
                            modifier = Modifier.size(17.dp),
                        )
                        Text(retryLabel, color = Paper, style = MaterialTheme.typography.labelLarge)
                    }
                }
            }
        }
    }
}

@Composable
private fun EndedOverlay(
    anime: Anime,
    episode: Episode,
    nextEpisode: Episode?,
    onReplay: () -> Unit,
    onNext: (() -> Unit)?,
    onBack: () -> Unit,
) {
    Box(
        Modifier.fillMaxSize().background(Color(0xD9080B10)),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            color = Color(0xFF141A26),
            shape = RoundedCornerShape(24.dp),
            modifier = Modifier.padding(28.dp),
        ) {
            Column(Modifier.padding(22.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Text(
                    "EPISODE COMPLETE",
                    color = Mint,
                    style = MaterialTheme.typography.labelMedium,
                    letterSpacing = 1.8.sp,
                )
                Text(
                    anime.title,
                    color = Paper,
                    style = MaterialTheme.typography.headlineSmall,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    episode.displayTitle(),
                    color = Muted,
                    style = MaterialTheme.typography.bodyMedium,
                )
                if (nextEpisode != null) {
                    Surface(
                        color = Color.White.copy(alpha = 0.055f),
                        shape = RoundedCornerShape(16.dp),
                    ) {
                        Row(
                            Modifier.fillMaxWidth().padding(13.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            Icon(Icons.Rounded.SkipNext, contentDescription = null, tint = Ice)
                            Column(Modifier.weight(1f)) {
                                Text(
                                    "UP NEXT",
                                    color = Muted,
                                    style = MaterialTheme.typography.labelSmall,
                                    letterSpacing = 1.2.sp,
                                )
                                Text(
                                    nextEpisode.displayTitle(),
                                    color = Paper,
                                    style = MaterialTheme.typography.titleMedium,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                    }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(9.dp)) {
                    if (nextEpisode != null && onNext != null) {
                        PrimaryAction(
                            text = "Play next",
                            modifier = Modifier.weight(1f),
                            onClick = onNext,
                        )
                    } else {
                        PrimaryAction(
                            text = "Replay",
                            modifier = Modifier.weight(1f),
                            onClick = onReplay,
                        )
                    }
                    GhostAction(text = "Exit", onClick = onBack)
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AudioSheet(
    audios: List<String>,
    currentSpec: String,
    currentAudio: String,
    activeTrack: String?,
    fallback: Boolean,
    onDismiss: () -> Unit,
    onSelect: (String) -> Unit,
) {
    val labels = audios.map { it.lowercase() }
    val known = labels.isNotEmpty()
    val hasSub = !known || labels.any { it == "sub" || it == "sub & dub" || it == "both" }
    val hasDub = !known || labels.any { it == "dub" || it == "sub & dub" || it == "both" }
    val dualFile = hasSub && hasDub &&
        !labels.any { it == "sub" } && !labels.any { it == "dub" }

    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = Night) {
        Column(
            Modifier
                .padding(horizontal = 20.dp)
                .verticalScroll(rememberScrollState())
                .padding(bottom = 30.dp),
        ) {
            Text("Audio", color = Paper, style = MaterialTheme.typography.headlineSmall)
            Spacer(Modifier.height(5.dp))
            Text(
                "Choose the version you want. The source is re-resolved for your pick.",
                color = Muted,
                style = MaterialTheme.typography.bodySmall,
            )
            if (fallback) {
                Spacer(Modifier.height(12.dp))
                Surface(
                    color = Amber.copy(alpha = 0.09f),
                    shape = RoundedCornerShape(14.dp),
                    border = BorderStroke(1.dp, Amber.copy(alpha = 0.24f)),
                ) {
                    Text(
                        "${currentSpec.audioName()} audio is not offered by this source. " +
                            "Playback is using ${currentAudio.audioName()} so the episode still works.",
                        modifier = Modifier.padding(12.dp),
                        color = Amber,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
            Spacer(Modifier.height(14.dp))
            if (!activeTrack.isNullOrBlank()) {
                Text(
                    "Active player track: $activeTrack",
                    color = Ice,
                    style = MaterialTheme.typography.bodySmall,
                )
                Spacer(Modifier.height(8.dp))
            }
            AudioRow(
                title = "Sub",
                subtitle = if (hasSub) "Original audio · subtitles" else "Not offered by this source",
                available = hasSub,
                selected = currentSpec == "sub",
                onClick = { onSelect("sub") },
            )
            AudioRow(
                title = "Dub",
                subtitle = if (hasDub) "Dubbed audio" else "Not offered by this source",
                available = hasDub,
                selected = currentSpec == "dub",
                onClick = { onSelect("dub") },
            )
            if (dualFile) {
                Spacer(Modifier.height(8.dp))
                Text(
                    "This source serves one dual-audio file. Both choices use that file; " +
                        "switch the active track from the player's settings gear.",
                    color = Muted,
                    style = MaterialTheme.typography.bodySmall,
                )
            } else if (known && !hasSub && !hasDub) {
                Spacer(Modifier.height(8.dp))
                Text(
                    "This source did not report sub or dub variants. The player's settings " +
                        "gear may still expose its available tracks.",
                    color = Muted,
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}

@Composable
private fun AudioRow(
    title: String,
    subtitle: String,
    available: Boolean,
    selected: Boolean,
    onClick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp)
            .clip(RoundedCornerShape(13.dp))
            .background(if (selected) Ice.copy(alpha = 0.1f) else Color.Transparent)
            .clickable(enabled = available, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            Icons.Rounded.Translate,
            contentDescription = null,
            tint = when {
                selected -> Ice
                available -> Muted
                else -> Muted.copy(alpha = 0.35f)
            },
            modifier = Modifier.size(19.dp),
        )
        Column(Modifier.weight(1f)) {
            Text(
                title,
                color = when {
                    selected -> Ice
                    available -> Paper
                    else -> Muted.copy(alpha = 0.45f)
                },
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                subtitle,
                color = Muted.copy(alpha = if (available) 1f else 0.5f),
                style = MaterialTheme.typography.bodySmall,
            )
        }
        if (selected) {
            Icon(
                Icons.Rounded.Check,
                contentDescription = null,
                tint = if (available) Mint else Amber,
                modifier = Modifier.size(19.dp),
            )
        }
    }
}

private fun String.audioName(): String = when (lowercase()) {
    "sub" -> "Sub"
    "dub" -> "Dub"
    "both" -> "Sub & Dub"
    "raw" -> "Raw"
    else -> replaceFirstChar { it.uppercase() }
}

private fun audioChipLabel(requested: String, actual: String, fallback: Boolean): String {
    val value = if (fallback) actual else requested
    return when (value.lowercase()) {
        "sub" -> "SUB"
        "dub" -> "DUB"
        "both" -> "DUAL"
        "raw" -> "RAW"
        else -> value.uppercase()
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun QualitySheet(
    qualities: List<Int>,
    currentSpec: String?,
    currentQuality: Int?,
    onDismiss: () -> Unit,
    onSelect: (String) -> Unit,
    onAuto: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = Night) {
        Column(
            Modifier
                .padding(horizontal = 20.dp)
                .verticalScroll(rememberScrollState())
                .padding(bottom = 30.dp),
        ) {
            Text("Quality", color = Paper, style = MaterialTheme.typography.headlineSmall)
            Spacer(Modifier.height(5.dp))
            Text(
                "The engine prefers the highest available stream and honors your choice.",
                color = Muted,
                style = MaterialTheme.typography.bodySmall,
            )
            Spacer(Modifier.height(14.dp))
            QualityRow(
                title = "Auto",
                subtitle = "Best available${currentQuality?.let { " · ${it}p" } ?: ""}",
                selected = currentSpec == null,
                onClick = onAuto,
            )
            qualities.forEach { quality ->
                QualityRow(
                    title = "${quality}p",
                    subtitle = if (quality >= 1080) "Full HD" else if (quality >= 720) "HD" else "SD",
                    selected = currentSpec == quality.toString(),
                    onClick = { onSelect(quality.toString()) },
                )
            }
            if (qualities.isEmpty()) {
                Text(
                    "This source did not publish quality variants. Auto is using the best stream.",
                    color = Muted,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }
}

@Composable
private fun QualityRow(
    title: String,
    subtitle: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp)
            .clip(RoundedCornerShape(13.dp))
            .background(if (selected) Ice.copy(alpha = 0.1f) else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(
            Icons.Rounded.HighQuality,
            contentDescription = null,
            tint = if (selected) Ice else Muted,
            modifier = Modifier.size(19.dp),
        )
        Column(Modifier.weight(1f)) {
            Text(title, color = if (selected) Ice else Paper, style = MaterialTheme.typography.titleMedium)
            Text(subtitle, color = Muted, style = MaterialTheme.typography.bodySmall)
        }
        if (selected) {
            Icon(Icons.Rounded.Check, contentDescription = null, tint = Mint, modifier = Modifier.size(19.dp))
        }
    }
}

private const val BROWSER_USER_AGENT =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36"

private fun buildMediaSource(context: Context, playback: Playback): MediaSource {
    val headers = playback.headers.filterKeys { it.lowercase() != "user-agent" }
    val dataSourceFactory: DataSource.Factory = if (playback.url.startsWith("file:")) {
        DefaultDataSource.Factory(context)
    } else {
        DefaultHttpDataSource.Factory()
            .setUserAgent(BROWSER_USER_AGENT)
            .setDefaultRequestProperties(headers)
            .setConnectTimeoutMs(20_000)
            .setReadTimeoutMs(45_000)
            .setAllowCrossProtocolRedirects(true)
    }

    val mediaItem = MediaItem.Builder()
        .setUri(playback.url)
        .apply {
            playback.mimeType?.let { setMimeType(it) }
        }
        .build()

    val base: MediaSource = when (playback.kind.lowercase()) {
        "hls" -> HlsMediaSource.Factory(dataSourceFactory)
            .setAllowChunklessPreparation(true)
            .createMediaSource(mediaItem)
        "dash" -> DashMediaSource.Factory(dataSourceFactory)
            .createMediaSource(mediaItem)
        else -> ProgressiveMediaSource.Factory(
            dataSourceFactory,
            DefaultExtractorsFactory(),
        ).createMediaSource(mediaItem)
    }

    val subtitleSources = playback.subtitles
        .asSequence()
        .mapNotNull { subtitle ->
            val mime = subtitleMime(subtitle) ?: return@mapNotNull null
            val language = languageCode(subtitle.lang)
            val configuration = MediaItem.SubtitleConfiguration.Builder(Uri.parse(subtitle.url))
                .setMimeType(mime)
                .setLanguage(language)
                .setLabel(subtitle.lang.ifBlank { language })
                .setSelectionFlags(
                    if (subtitle.default || subtitle.lang.contains("english", ignoreCase = true)) {
                        C.SELECTION_FLAG_DEFAULT
                    } else {
                        0
                    },
                )
                .build()
            SingleSampleMediaSource.Factory(dataSourceFactory)
                .createMediaSource(configuration, C.TIME_UNSET)
        }
        .take(8)
        .toList()

    return if (subtitleSources.isEmpty()) {
        base
    } else {
        MergingMediaSource(base, *subtitleSources.toTypedArray())
    }
}

private fun subtitleMime(subtitle: Subtitle): String? {
    val format = subtitle.format?.lowercase().orEmpty()
    val extension = subtitle.url.substringBefore('?').substringAfterLast('.', "").lowercase()
    return when {
        format.contains("vtt") || extension == "vtt" -> MimeTypes.TEXT_VTT
        format.contains("srt") || format.contains("subrip") || extension == "srt" ->
            MimeTypes.APPLICATION_SUBRIP
        format.contains("ttml") || extension == "ttml" -> MimeTypes.APPLICATION_TTML
        else -> null
    }
}

private fun localMime(extension: String): String? = when (extension.lowercase()) {
    "mp4", "m4v" -> MimeTypes.VIDEO_MP4
    "mkv" -> MimeTypes.VIDEO_MATROSKA
    "webm" -> MimeTypes.VIDEO_WEBM
    "ts" -> MimeTypes.VIDEO_MP2T
    "mov" -> "video/quicktime"
    else -> null
}

private fun languageCode(language: String): String {
    val value = language.lowercase()
    return when {
        value.contains("english") || value == "en" -> "en"
        value.contains("spanish") || value == "es" -> "es"
        value.contains("french") || value == "fr" -> "fr"
        value.contains("german") || value == "de" -> "de"
        value.contains("portuguese") || value == "pt" -> "pt"
        value.contains("italian") || value == "it" -> "it"
        value.contains("russian") || value == "ru" -> "ru"
        value.contains("arabic") || value == "ar" -> "ar"
        value.contains("hindi") || value == "hi" -> "hi"
        value.contains("japanese") || value == "ja" -> "ja"
        else -> "und"
    }
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}