package io.github.nabeiroriviereadriano.nothingconnect.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
fun Panel(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier.fillMaxWidth().background(Nc.Surface, RoundedCornerShape(24.dp)).padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
        content = content,
    )
}

@Composable
fun StatusDot(live: Boolean) {
    Box(Modifier.size(8.dp).background(if (live) Nc.Red else Nc.Line, CircleShape))
}

@Composable
fun Primary(text: String, enabled: Boolean = true, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Button(onClick, modifier, enabled = enabled, colors = ButtonDefaults.buttonColors(containerColor = Nc.White, contentColor = Nc.Black)) { Text(text) }
}

@Composable
fun Secondary(text: String, enabled: Boolean = true, danger: Boolean = false, modifier: Modifier = Modifier, onClick: () -> Unit) {
    OutlinedButton(onClick, modifier, enabled = enabled, colors = ButtonDefaults.outlinedButtonColors(contentColor = if (danger) Nc.Red else Nc.White)) { Text(text) }
}

fun bytes(n: Long): String {
    val u = listOf("o", "Ko", "Mo", "Go", "To")
    var v = n.toDouble()
    var i = 0
    while (v >= 1000 && i < u.size - 1) { v /= 1000; i++ }
    return (if (v < 10 && i > 0) "%.1f".format(v) else "%.0f".format(v)) + " " + u[i]
}

val STATE_LABEL = mapOf(
    "offered" to "En attente du PC", "waiting" to "À accepter", "running" to "En cours", "paused" to "En pause",
    "interrupted" to "Interrompu, reprise à la reconnexion", "completed" to "Terminé", "failed" to "Échec",
    "cancelled" to "Annulé", "rejected" to "Refusé",
)
