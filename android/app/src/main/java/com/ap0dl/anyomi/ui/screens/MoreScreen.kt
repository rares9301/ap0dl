package com.ap0dl.anyomi.ui.screens

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.DeleteOutline
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Translate
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ap0dl.anyomi.AppViewModel
import com.ap0dl.anyomi.BuildConfig
import com.ap0dl.anyomi.R
import com.ap0dl.anyomi.data.Loadable
import com.ap0dl.anyomi.ui.components.LogoMark
import com.ap0dl.anyomi.ui.components.ProviderRow
import com.ap0dl.anyomi.ui.components.SectionHeading
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.NightCard
import com.ap0dl.anyomi.ui.theme.NightLine
import com.ap0dl.anyomi.ui.theme.Paper

@Composable
fun MoreScreen(app: AppViewModel) {
    val providers by app.providers.collectAsStateWithLifecycle()
    val history by app.history.collectAsStateWithLifecycle()
    val audioPreference by app.audioPreference.collectAsStateWithLifecycle()
    var confirmClear by remember { mutableStateOf(false) }

    LazyColumn(
        modifier = Modifier.fillMaxWidth().background(Night).statusBarsPadding(),
        contentPadding = PaddingValues(bottom = 32.dp),
    ) {
        item {
            Column(Modifier.padding(horizontal = 20.dp).padding(top = 18.dp)) {
                Text(
                    "MORE",
                    color = Mint,
                    style = MaterialTheme.typography.labelMedium,
                    letterSpacing = 2.sp,
                )
                Text(
                    "Settings",
                    color = Paper,
                    style = MaterialTheme.typography.headlineLarge,
                )
                Spacer(Modifier.height(18.dp))
                Surface(
                    color = Color.Transparent,
                    shape = RoundedCornerShape(24.dp),
                    border = BorderStroke(1.dp, NightLine.copy(alpha = 0.65f)),
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(
                                Brush.linearGradient(
                                    listOf(Color(0xFF21345C), Color(0xFF141A27)),
                                ),
                            )
                            .padding(18.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        LogoMark(size = 56)
                        Column(Modifier.weight(1f)) {
                            Text(
                                "AP0DL",
                                color = Paper,
                                style = MaterialTheme.typography.headlineSmall,
                                letterSpacing = 1.2.sp,
                                fontWeight = FontWeight.Black,
                            )
                            Text(
                                "Version ${BuildConfig.VERSION_NAME} · Android",
                                color = Muted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                            Spacer(Modifier.height(3.dp))
                            Text(
                                (providers as? Loadable.Ready)?.value?.size
                                    ?.let { "$it live sources" }
                                    ?: "Live sources",
                                color = Ice,
                                style = MaterialTheme.typography.labelMedium,
                            )
                        }
                    }
                }
            }
        }
        item {
            Column(Modifier.padding(horizontal = 20.dp).padding(top = 26.dp)) {
                SectionHeading(
                    title = "Sources",
                    eyebrow = "Live adapters",
                    action = "Refresh",
                    onAction = { app.refreshProviders(force = true) },
                )
                Spacer(Modifier.height(6.dp))
                Surface(
                    color = NightCard.copy(alpha = 0.62f),
                    shape = RoundedCornerShape(20.dp),
                    border = BorderStroke(1.dp, NightLine.copy(alpha = 0.6f)),
                ) {
                    Column(Modifier.padding(horizontal = 14.dp, vertical = 2.dp)) {
                        when (val state = providers) {
                            Loadable.Loading, Loadable.Idle -> Row(
                                Modifier.padding(vertical = 18.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text("Loading providers…", color = Muted, style = MaterialTheme.typography.bodyMedium)
                            }
                            is Loadable.Failed -> Text(
                                state.message,
                                modifier = Modifier.padding(vertical = 16.dp),
                                color = Muted,
                                style = MaterialTheme.typography.bodyMedium,
                            )
                            is Loadable.Ready -> {
                                state.value.forEach { provider ->
                                    ProviderRow(provider)
                                    if (provider != state.value.last()) {
                                        Spacer(
                                            Modifier
                                                .fillMaxWidth()
                                                .height(1.dp)
                                                .background(NightLine.copy(alpha = 0.5f)),
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        item {
            Column(Modifier.padding(horizontal = 20.dp).padding(top = 26.dp)) {
                SectionHeading(title = "Audio", eyebrow = "Playback default")
                Spacer(Modifier.height(10.dp))
                Surface(
                    color = NightCard.copy(alpha = 0.62f),
                    shape = RoundedCornerShape(18.dp),
                    border = BorderStroke(1.dp, NightLine.copy(alpha = 0.6f)),
                ) {
                    Column(Modifier.padding(14.dp)) {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(9.dp),
                        ) {
                            AudioChoice(
                                title = "Sub",
                                subtitle = "Original · subtitles",
                                selected = audioPreference != "dub",
                                modifier = Modifier.weight(1f),
                                onClick = { app.setAudioPreference("sub") },
                            )
                            AudioChoice(
                                title = "Dub",
                                subtitle = "Dubbed audio",
                                selected = audioPreference == "dub",
                                modifier = Modifier.weight(1f),
                                onClick = { app.setAudioPreference("dub") },
                            )
                        }
                        Spacer(Modifier.height(11.dp))
                        Text(
                            "Dubs are never hidden. This picks the version playback and downloads " +
                                "try first, and you can switch from the AUDIO chip in the player.",
                            color = Muted,
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                }
            }
        }
        item {
            Column(Modifier.padding(horizontal = 20.dp).padding(top = 26.dp)) {
                SectionHeading(title = "Team", eyebrow = "Who built this")
                Spacer(Modifier.height(10.dp))
                Surface(
                    color = NightCard.copy(alpha = 0.62f),
                    shape = RoundedCornerShape(20.dp),
                    border = BorderStroke(1.dp, NightLine.copy(alpha = 0.6f)),
                ) {
                    Column(Modifier.padding(horizontal = 14.dp, vertical = 3.dp)) {
                        TeamRow(
                            initials = "RS",
                            name = "Rares-Andrei Sarmasag",
                            handle = "@rares9301",
                            role = "Product & app",
                            accent = Ice,
                        )
                        MemberDivider()
                        TeamRow(
                            initials = "VI",
                            name = "Victor Iusco",
                            handle = "@Victor-Iusco",
                            role = "Android & UI",
                            accent = Mint,
                        )
                        MemberDivider()
                        TeamRow(
                            initials = "DS",
                            name = "DeepSeek V4.1 Flash",
                            handle = "release 1051",
                            role = "Rust engine & implementation",
                            accent = Color(0xFF8FB4FF),
                        )
                        MemberDivider()
                        TeamRow(
                            initials = "CO",
                            name = "Claude Opus 5.5",
                            handle = "Anthropic",
                            role = "Design & code review",
                            accent = Color(0xFFC5A6FF),
                        )
                    }
                }
                Spacer(Modifier.height(22.dp))
                BuiltWithCard()
                Spacer(Modifier.height(12.dp))
                TokenCard()
            }
        }
        item {
            Column(Modifier.padding(horizontal = 20.dp).padding(top = 26.dp)) {
                SectionHeading(title = "Watch history", eyebrow = "Stored on this device")
                Spacer(Modifier.height(10.dp))
                Surface(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(18.dp))
                        .clickable(enabled = history.isNotEmpty()) { confirmClear = true },
                    color = NightCard.copy(alpha = 0.62f),
                    shape = RoundedCornerShape(18.dp),
                    border = BorderStroke(1.dp, NightLine.copy(alpha = 0.6f)),
                ) {
                    Row(
                        Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(11.dp),
                    ) {
                        Icon(
                            Icons.Rounded.DeleteOutline,
                            contentDescription = null,
                            tint = if (history.isEmpty()) Muted else Ice,
                            modifier = Modifier.size(21.dp),
                        )
                        Column(Modifier.weight(1f)) {
                            Text(
                                if (history.isEmpty()) "No history yet" else "Clear ${history.size} entries",
                                color = if (history.isEmpty()) Muted else Paper,
                                style = MaterialTheme.typography.titleMedium,
                            )
                            Text(
                                "Continue watching is never uploaded.",
                                color = Muted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                }
            }
        }
        item {
            Column(Modifier.padding(horizontal = 20.dp).padding(top = 26.dp)) {
                SectionHeading(title = "Engine", eyebrow = "Inside the app")
                Spacer(Modifier.height(10.dp))
                EngineInfoRow("Engine", "ap0dl-core 2.2.0")
                EngineInfoRow("Runtime", "Rust async + tokio")
                EngineInfoRow("Video", "Media3 / ExoPlayer")
                EngineInfoRow("Providers", "Six adapters bundled")
                EngineInfoRow("Network", "Native TLS · live extraction")
            }
        }
        item {
            Column(Modifier.padding(horizontal = 20.dp).padding(top = 26.dp)) {
                Text(
                    "AP0DL does not host or upload video. All media is resolved from third-party providers and remains their responsibility. Use this app only where the content is legal for you to access.",
                    color = Muted.copy(alpha = 0.72f),
                    style = MaterialTheme.typography.bodySmall,
                )
                Spacer(Modifier.height(18.dp))
                Text(
                    "AP0DL · MADE WITH A RUST HEART",
                    color = Muted.copy(alpha = 0.45f),
                    style = MaterialTheme.typography.labelSmall,
                    letterSpacing = 1.1.sp,
                )
            }
        }
    }

    if (confirmClear) {
        AlertDialog(
            onDismissRequest = { confirmClear = false },
            title = { Text("Clear watch history?") },
            text = { Text("Continue watching entries will be removed from this device.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        app.clearHistory()
                        confirmClear = false
                    },
                ) { Text("Clear", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = {
                TextButton(onClick = { confirmClear = false }) { Text("Cancel") }
            },
            containerColor = NightCard,
        )
    }
}

@Composable
private fun TeamRow(
    initials: String,
    name: String,
    handle: String,
    role: String,
    accent: Color,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(11.dp),
    ) {
        Box(
            modifier = Modifier
                .size(38.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(accent.copy(alpha = 0.13f)),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                initials,
                color = accent,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Black,
            )
        }
        Column(Modifier.weight(1f)) {
            Text(
                name,
                color = Paper,
                style = MaterialTheme.typography.titleSmall,
                maxLines = 1,
            )
            Text(
                "$role · $handle",
                color = Muted,
                style = MaterialTheme.typography.bodySmall,
                maxLines = 1,
            )
        }
    }
}

@Composable
private fun MemberDivider() {
    Box(
        Modifier
            .fillMaxWidth()
            .height(1.dp)
            .background(NightLine.copy(alpha = 0.5f)),
    )
}

@Composable
private fun BuiltWithCard() {
    Surface(
        color = Color.Transparent,
        shape = RoundedCornerShape(22.dp),
        border = BorderStroke(1.dp, Ice.copy(alpha = 0.16f)),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(
                    Brush.linearGradient(
                        listOf(Color(0xFF101A2C), Color(0xFF141722)),
                    ),
                )
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(82.dp)
                    .clip(RoundedCornerShape(24.dp))
                    .background(Color(0xFF0B1220)),
                contentAlignment = Alignment.Center,
            ) {
                Image(
                    painter = painterResource(R.drawable.ic_launcher_foreground),
                    contentDescription = "AP0DL crest",
                    modifier = Modifier.size(74.dp),
                    contentScale = ContentScale.Fit,
                )
            }
            Column(Modifier.weight(1f)) {
                Text(
                    "AP0DL crest",
                    color = Paper,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                )
                Spacer(Modifier.height(3.dp))
                Text(
                    "Arch Linux · Rust · DeepSeek V4.1 Flash · Politehnica Bucharest",
                    color = Muted,
                    style = MaterialTheme.typography.bodySmall,
                )
                Spacer(Modifier.height(5.dp))
                Text(
                    "One player. Four influences.",
                    color = Ice,
                    style = MaterialTheme.typography.labelSmall,
                )
            }
        }
    }
}

@Composable
private fun TokenCard() {
    Surface(
        color = Color.Transparent,
        shape = RoundedCornerShape(22.dp),
        border = BorderStroke(1.dp, Color(0xFF6E5AA8).copy(alpha = 0.38f)),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(
                    Brush.linearGradient(
                        listOf(Color(0xFF2A2345), Color(0xFF141722)),
                    ),
                )
                .padding(18.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "TOKENS INCINERATED",
                    color = Color(0xFFC5A6FF),
                    style = MaterialTheme.typography.labelSmall,
                    letterSpacing = 1.3.sp,
                )
                Text("✦", color = Mint, style = MaterialTheme.typography.titleMedium)
            }
            Spacer(Modifier.height(8.dp))
            Text(
                "294M",
                color = Paper,
                style = MaterialTheme.typography.displayLarge,
                fontWeight = FontWeight.Black,
            )
            Text(
                "tokens spent building AP0DL",
                color = Muted,
                style = MaterialTheme.typography.bodyMedium,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                "Enough context to rewrite the app twice. We only did it once. :)",
                color = Color(0xFFC5A6FF).copy(alpha = 0.8f),
                style = MaterialTheme.typography.bodySmall,
            )
        }
    }
}

@Composable
private fun AudioChoice(
    title: String,
    subtitle: String,
    selected: Boolean,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    Surface(
        modifier = modifier
            .clip(RoundedCornerShape(14.dp))
            .clickable(onClick = onClick),
        color = if (selected) Ice.copy(alpha = 0.1f) else Color.White.copy(alpha = 0.035f),
        shape = RoundedCornerShape(14.dp),
        border = BorderStroke(
            1.dp,
            if (selected) Ice.copy(alpha = 0.42f) else NightLine.copy(alpha = 0.55f),
        ),
    ) {
        Column(Modifier.padding(horizontal = 12.dp, vertical = 11.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(7.dp),
            ) {
                Icon(
                    Icons.Rounded.Translate,
                    contentDescription = null,
                    tint = if (selected) Ice else Muted,
                    modifier = Modifier.size(17.dp),
                )
                Text(
                    title,
                    color = if (selected) Ice else Paper,
                    style = MaterialTheme.typography.titleMedium,
                )
                if (selected) {
                    Icon(
                        Icons.Rounded.Check,
                        contentDescription = null,
                        tint = Mint,
                        modifier = Modifier.size(16.dp),
                    )
                }
            }
            Spacer(Modifier.height(2.dp))
            Text(subtitle, color = Muted, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun EngineInfoRow(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 7.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = Muted, style = MaterialTheme.typography.bodyMedium)
        Text(
            value,
            color = Paper.copy(alpha = 0.9f),
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium,
        )
    }
}