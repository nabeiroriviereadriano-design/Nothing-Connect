package io.github.nabeiroriviereadriano.nothingconnect.stream

import android.accessibilityservice.AccessibilityService
import android.accessibilityservice.GestureDescription
import android.graphics.Path
import android.graphics.Point
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import org.json.JSONObject

/**
 * Contrôle du téléphone depuis le PC pendant le miroir (toucher, balayer, touches système,
 * saisie de texte). Service d'accessibilité activé explicitement par l'utilisateur dans
 * Paramètres › Accessibilité ; il ne lit aucun contenu et n'agit que sur commande du PC associé.
 */
class ControlService : AccessibilityService() {

    override fun onServiceConnected() {
        instance = this
        Core.announceCapabilities()
    }

    override fun onDestroy() {
        if (instance === this) instance = null
        Core.announceCapabilities()
        super.onDestroy()
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {}
    override fun onInterrupt() {}

    private fun screenSize(): Point {
        val wm = getSystemService(WindowManager::class.java)
        return if (Build.VERSION.SDK_INT >= 30) {
            val b = wm.maximumWindowMetrics.bounds
            Point(b.width(), b.height())
        } else {
            Point().also { @Suppress("DEPRECATION") wm.defaultDisplay.getRealSize(it) }
        }
    }

    private fun gesture(path: Path, durationMs: Long) {
        val g = GestureDescription.Builder()
            .addStroke(GestureDescription.StrokeDescription(path, 0, durationMs.coerceIn(1, 5000)))
            .build()
        dispatchGesture(g, null, null)
    }

    fun handle(ev: JSONObject) {
        val s = screenSize()
        fun px(k: String) = (ev.getDouble(k).toFloat().coerceIn(0f, 1f) * (s.x - 1))
        fun py(k: String) = (ev.getDouble(k).toFloat().coerceIn(0f, 1f) * (s.y - 1))
        when (ev.getString("i")) {
            "tap" -> gesture(Path().apply { moveTo(px("x"), py("y")) }, 40)
            "long_press" -> gesture(Path().apply { moveTo(px("x"), py("y")) }, 700)
            "swipe" -> gesture(Path().apply { moveTo(px("x1"), py("y1")); lineTo(px("x2"), py("y2")) }, ev.optLong("duration_ms", 200))
            "key" -> performGlobalAction(
                when (ev.getString("key")) {
                    "back" -> GLOBAL_ACTION_BACK
                    "home" -> GLOBAL_ACTION_HOME
                    "recents" -> GLOBAL_ACTION_RECENTS
                    else -> GLOBAL_ACTION_NOTIFICATIONS
                }
            )
            "text" -> typeText(ev.getString("text"))
        }
    }

    /** Ajoute le texte au champ qui a le focus (le texte existant est conservé). */
    private fun typeText(text: String) {
        val node = rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) ?: return
        val current = if (node.isShowingHintText) "" else node.text?.toString().orEmpty()
        val args = Bundle().apply {
            putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, current + text)
        }
        node.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)
    }

    companion object {
        @Volatile
        var instance: ControlService? = null
    }
}
