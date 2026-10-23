package com.ap0dl.anyomi.ui.components

import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ArrowForward
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material.icons.rounded.Refresh
import androidx.compose.material.icons.rounded.Search
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ap0dl.anyomi.ui.theme.Amber
import com.ap0dl.anyomi.ui.theme.Ice
import com.ap0dl.anyomi.ui.theme.Mint
import com.ap0dl.anyomi.ui.theme.Muted
import com.ap0dl.anyomi.ui.theme.Night
import com.ap0dl.anyomi.ui.theme.NightCard
import com.ap0dl.anyomi.ui.theme.NightLine
import com.ap0dl.anyomi.ui.theme.Paper

fun providerColor(id: String): Color = when (id.lowercase()) {
    "reanime" -> Color(0xFF8EA8FF)
    "animegg" -> Color(0xFF71C8FF)
    "anidb" -> Color(0xFFF5BE6A)
    "kisskh" -> Color(0xFFFF91A7)
    "animeparadise" -> Color(0xFFC59BFF)
    "animetoki" -> Color(0xFF63E6C4)
    else -> Ice
}

fun providerLabel(id: String, fallback: String = id): String = when (id.lowercase()) {
    "reanime" -> "Re:Anime"
    "animegg" -> "AnimeGG"
    "anidb" -> "AniDB"
    "kisskh" -> "KissKH"
    "animeparadise" -> "AnimeParadise"
    "animetoki" -> "AnimeToki"
    else -> fallback
}

fun statusColor(status: String): Color = when (status.lowercase()) {
    "stable" -> Mint
    "experimental" -> Amber
    else -> Muted
}

fun metaLine(vararg values: String?): String =
    values.filterNot { it.isNullOrBlank() || it == "0" }.joinToString("  ·  ")

@Composable
fun LogoMark(modifier: Modifier = Modifier, size: Int = 34) {
    Box(
        modifier = modifier
            .size(size.dp)
            .clip(CircleShape)
            .background(Brush.linearGradient(listOf(Color(0xFF2C4D9C), Color(0xFF14233F))))
            .border(1.dp, Ice.copy(alpha = 0.55f), CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(
            imageVector = Icons.Rounded.PlayArrow,
            contentDescription = null,
            tint = Mint,
            modifier = Modifier.size((size * 0.62).dp),
        )
    }
}

@Composable
fun SourcePill(
    providerId: String,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val accent = providerColor(providerId)
    Row(
        modifier = modifier
            .clip(RoundedCornerShape(50))
            .background(Color.Black.copy(alpha = 0.5f))
            .border(1.dp, accent.copy(alpha = 0.38f), RoundedCornerShape(50))
            .padding(horizontal = if (compact) 8.dp else 10.dp, vertical = if (compact) 3.dp else 5.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        Box(Modifier.size(5.dp).clip(CircleShape).background(accent))
        Text(
            text = providerLabel(providerId),
            color = accent,
            style = if (compact) MaterialTheme.typography.labelSmall else MaterialTheme.typography.labelMedium,
            maxLines = 1,
        )
    }
}

@Composable
fun MetaPill(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text.uppercase(),
        modifier = modifier
            .clip(RoundedCornerShape(7.dp))
            .background(Color.White.copy(alpha = 0.08f))
            .padding(horizontal = 8.dp, vertical = 4.dp),
        color = Paper.copy(alpha = 0.88f),
        style = MaterialTheme.typography.labelSmall,
        fontWeight = FontWeight.SemiBold,
    )
}

@Composable
fun SectionHeading(
    title: String,
    modifier: Modifier = Modifier,
    eyebrow: String? = null,
    action: String? = null,
    onAction: (() -> Unit)? = null,
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        verticalAlignment = Alignment.Bottom,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Column {
            if (eyebrow != null) {
                Text(
                    eyebrow.uppercase(),
                    color = Ice.copy(alpha = 0.72f),
                    style = MaterialTheme.typography.labelMedium,
                    letterSpacing = 1.4.sp,
                )
                Spacer(Modifier.height(3.dp))
            }
            Text(
                title,
                style = MaterialTheme.typography.headlineSmall,
                color = Paper,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (action != null && onAction != null) {
            Row(
                modifier = Modifier
                    .clip(RoundedCornerShape(50))
                    .clickable(onClick = onAction)
                    .padding(horizontal = 8.dp, vertical = 5.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(3.dp),
            ) {
                Text(action, color = Ice, style = MaterialTheme.typography.labelLarge)
                Icon(
                    Icons.Rounded.ArrowForward,
                    contentDescription = null,
                    tint = Ice,
                    modifier = Modifier.size(16.dp),
                )
            }
        }
    }
}

@Composable
fun ReadOnlySearchBar(
    hint: String,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    Surface(
        modifier = modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).clickable(onClick = onClick),
        color = NightCard.copy(alpha = 0.92f),
        shape = RoundedCornerShape(18.dp),
        border = BorderStroke(1.dp, NightLine.copy(alpha = 0.8f)),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(Icons.Rounded.Search, contentDescription = null, tint = Ice, modifier = Modifier.size(21.dp))
            Text(hint, color = Muted, style = MaterialTheme.typography.bodyLarge)
        }
    }
}

@Composable
fun SearchInput(
    value: String,
    onValueChange: (String) -> Unit,
    onSearch: () -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "Search every source…",
    autoFocus: Boolean = false,
) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        color = NightCard.copy(alpha = 0.94f),
        shape = RoundedCornerShape(18.dp),
        border = BorderStroke(1.dp, NightLine.copy(alpha = 0.85f)),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 15.dp, vertical = 13.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(11.dp),
        ) {
            Icon(Icons.Rounded.Search, contentDescription = null, tint = Ice, modifier = Modifier.size(21.dp))
            BasicTextField(
                value = value,
                onValueChange = onValueChange,
                modifier = Modifier.weight(1f),
                singleLine = true,
                textStyle = MaterialTheme.typography.bodyLarge.copy(color = Paper),
                cursorBrush = SolidColor(Ice),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { onSearch() }),
                decorationBox = { inner ->
                    Box {
                        if (value.isEmpty()) {
                            Text(placeholder, color = Muted, style = MaterialTheme.typography.bodyLarge)
                        }
                        inner()
                    }
                },
            )
            if (value.isNotEmpty()) {
                IconButton(onClick = { onValueChange("") }, modifier = Modifier.size(28.dp)) {
                    Icon(
                        Icons.Rounded.Close,
                        contentDescription = "Clear search",
                        tint = Muted,
                        modifier = Modifier.size(18.dp),
                    )
                }
            }
        }
    }
}

@Composable
fun PrimaryAction(
    text: String,
    modifier: Modifier = Modifier,
    icon: Boolean = true,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    Button(
        onClick = onClick,
        modifier = modifier.height(48.dp),
        enabled = enabled,
        shape = RoundedCornerShape(14.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = Ice,
            contentColor = Night,
            disabledContainerColor = Ice.copy(alpha = 0.3f),
            disabledContentColor = Night.copy(alpha = 0.55f),
        ),
        contentPadding = ButtonDefaults.ContentPadding,
    ) {
        if (icon) {
            Icon(
                Icons.Rounded.PlayArrow,
                contentDescription = null,
                modifier = Modifier.size(20.dp),
            )
            Spacer(Modifier.width(7.dp))
        }
        Text(text, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.Bold)
    }
}

@Composable
fun GhostAction(
    text: String,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    Surface(
        modifier = modifier.height(48.dp).clickable(onClick = onClick),
        color = Color.White.copy(alpha = 0.06f),
        shape = RoundedCornerShape(14.dp),
        border = BorderStroke(1.dp, Color.White.copy(alpha = 0.12f)),
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 18.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text, color = Paper, style = MaterialTheme.typography.labelLarge)
        }
    }
}

@Composable
fun ErrorPanel(
    message: String,
    modifier: Modifier = Modifier,
    onRetry: (() -> Unit)? = null,
) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        color = MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.28f),
        shape = RoundedCornerShape(18.dp),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.error.copy(alpha = 0.25f)),
    ) {
        Column(
            modifier = Modifier.padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text("Something went wrong", style = MaterialTheme.typography.titleMedium, color = Paper)
            Text(message, style = MaterialTheme.typography.bodyMedium, color = Muted)
            if (onRetry != null) {
                Row(
                    modifier = Modifier
                        .clip(RoundedCornerShape(10.dp))
                        .background(Color.White.copy(alpha = 0.08f))
                        .clickable(onClick = onRetry)
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Icon(
                        Icons.Rounded.Refresh,
                        contentDescription = null,
                        tint = Ice,
                        modifier = Modifier.size(17.dp),
                    )
                    Text("Try again", color = Ice, style = MaterialTheme.typography.labelLarge)
                }
            }
        }
    }
}

@Composable
fun EmptyPanel(
    title: String,
    message: String,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.fillMaxWidth().padding(vertical = 30.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(
            Modifier.size(52.dp).clip(CircleShape).background(Ice.copy(alpha = 0.1f)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(Icons.Rounded.Search, contentDescription = null, tint = Ice)
        }
        Text(title, style = MaterialTheme.typography.titleMedium, color = Paper)
        Text(
            message,
            style = MaterialTheme.typography.bodyMedium,
            color = Muted,
            textAlign = androidx.compose.ui.text.style.TextAlign.Center,
        )
    }
}

@Composable
fun SkeletonBox(
    modifier: Modifier = Modifier,
    corner: Int = 16,
) {
    val transition = rememberInfiniteTransition(label = "skeleton")
    val alpha by transition.animateFloat(
        initialValue = 0.35f,
        targetValue = 0.75f,
        animationSpec = infiniteRepeatable(
            animation = tween(900),
            repeatMode = RepeatMode.Reverse,
        ),
        label = "skeleton-alpha",
    )
    Box(
        modifier = modifier
            .clip(RoundedCornerShape(corner.dp))
            .background(Color.White.copy(alpha = alpha * 0.09f)),
    )
}

@Composable
fun SkeletonPoster(modifier: Modifier = Modifier) {
    Column(modifier = modifier) {
        SkeletonBox(modifier = Modifier.fillMaxWidth().aspectRatio(2f / 3f), corner = 18)
        Spacer(Modifier.height(10.dp))
        SkeletonBox(modifier = Modifier.fillMaxWidth(0.88f).height(14.dp), corner = 6)
        Spacer(Modifier.height(6.dp))
        SkeletonBox(modifier = Modifier.fillMaxWidth(0.55f).height(10.dp), corner = 5)
    }
}

