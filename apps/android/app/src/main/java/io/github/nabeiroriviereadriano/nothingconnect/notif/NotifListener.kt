package io.github.nabeiroriviereadriano.nothingconnect.notif

import android.app.ActivityOptions
import android.app.Notification
import android.app.PendingIntent
import android.app.RemoteInput
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Build
import android.os.Bundle
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import android.util.Base64
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.util.concurrent.ConcurrentHashMap

/**
 * Lit les notifications (après activation explicite « Accès aux notifications »).
 * Le filtrage par application est appliqué ICI, avant toute transmission au PC.
 */
class NotifListener : NotificationListenerService() {
    private val icons = ConcurrentHashMap<String, String>()

    override fun onListenerConnected() {
        instance = this
        MediaWatcher.ensure(this)
        sendAppList()
    }

    override fun onListenerDisconnected() {
        if (instance === this) instance = null
    }

    override fun onNotificationPosted(sbn: StatusBarNotification) {
        if (!shouldForward(sbn)) return
        Core.broadcast(toJson(sbn))
    }

    override fun onNotificationRemoved(sbn: StatusBarNotification) {
        if (sbn.packageName == packageName) return
        Core.broadcast(JSONObject().put("t", "notification_removed").put("key", sbn.key))
    }

    private fun shouldForward(sbn: StatusBarNotification): Boolean {
        val n = sbn.notification
        if (!Core.prefs.shareNotifications || Core.connected.value.isEmpty()) return false
        if (sbn.packageName == packageName) return false
        if (Core.prefs.isAppBlocked(sbn.packageName)) return false
        if (sbn.isOngoing || n.flags and Notification.FLAG_GROUP_SUMMARY != 0) return false
        if (n.category == Notification.CATEGORY_TRANSPORT || n.category == Notification.CATEGORY_PROGRESS) return false
        val e = n.extras
        return !(e.getCharSequence(Notification.EXTRA_TITLE).isNullOrBlank() && e.getCharSequence(Notification.EXTRA_TEXT).isNullOrBlank())
    }

    private fun toJson(sbn: StatusBarNotification): JSONObject {
        val n = sbn.notification
        val e = n.extras
        val text = e.getCharSequence(Notification.EXTRA_BIG_TEXT) ?: e.getCharSequence(Notification.EXTRA_TEXT)
        val actions = JSONArray()
        n.actions?.forEachIndexed { i, a ->
            if (a.actionIntent != null && !a.title.isNullOrBlank()) {
                actions.put(JSONObject().put("index", i).put("title", a.title.toString()).put("accepts_text", !a.remoteInputs.isNullOrEmpty()))
            }
        }
        return JSONObject()
            .put("t", "notification_posted")
            .put("key", sbn.key)
            .put("package", sbn.packageName)
            .put("app_name", appName(sbn.packageName))
            .put("title", e.getCharSequence(Notification.EXTRA_TITLE)?.toString() ?: JSONObject.NULL)
            .put("text", text?.toString()?.take(2000) ?: JSONObject.NULL)
            .put("posted_at", sbn.postTime)
            .put("icon_png_b64", icon(sbn.packageName) ?: JSONObject.NULL)
            .put("actions", actions)
            .put("sensitive", n.visibility != Notification.VISIBILITY_PUBLIC && n.publicVersion != null || n.visibility == Notification.VISIBILITY_SECRET)
    }

    private fun appName(pkg: String): String = runCatching {
        packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString()
    }.getOrDefault(pkg)

    /** Icône de l'appli en PNG 48 px, mise en cache (quelques Ko par appli). */
    private fun icon(pkg: String): String? = icons.getOrPut(pkg) {
        runCatching {
            val d = packageManager.getApplicationIcon(pkg)
            val bmp = Bitmap.createBitmap(48, 48, Bitmap.Config.ARGB_8888)
            val c = Canvas(bmp)
            d.setBounds(0, 0, 48, 48)
            d.draw(c)
            val out = ByteArrayOutputStream()
            bmp.compress(Bitmap.CompressFormat.PNG, 100, out)
            Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
        }.getOrDefault("")
    }.takeIf { it.isNotEmpty() }

    /** Renvoie les notifications visibles au PC qui vient de se connecter. */
    fun resendActive(peer: String) {
        runCatching { activeNotifications }.getOrNull()?.filter { shouldForward(it) }?.forEach { Core.send(peer, toJson(it)) }
    }

    /** Liste des applications (lançables + vues dans les notifications) avec l'état du filtre. */
    fun sendAppList() {
        if (Core.connected.value.isEmpty()) return
        val pm = packageManager
        val launchable = pm.queryIntentActivities(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER), PackageManager.MATCH_ALL)
            .map { it.activityInfo.packageName }
        val active = runCatching { activeNotifications.map { it.packageName } }.getOrDefault(emptyList())
        val apps = JSONArray()
        (launchable + active).distinct().filter { it != packageName }.map { it to appName(it) }.sortedBy { it.second.lowercase() }
            .forEach { (pkg, name) -> apps.put(JSONObject().put("package", pkg).put("app_name", name).put("enabled", !Core.prefs.isAppBlocked(pkg))) }
        Core.broadcast(JSONObject().put("t", "notification_apps").put("apps", apps))
    }

    private fun find(key: String): StatusBarNotification? = runCatching { activeNotifications.firstOrNull { it.key == key } }.getOrNull()

    fun perform(key: String, index: Int, reply: String?) {
        val sbn = find(key) ?: return
        val action = sbn.notification.actions?.getOrNull(index) ?: return
        val intent = Intent()
        if (reply != null && !action.remoteInputs.isNullOrEmpty()) {
            val results = Bundle()
            action.remoteInputs.forEach { results.putCharSequence(it.resultKey, reply) }
            RemoteInput.addResultsToIntent(action.remoteInputs, intent, results)
        }
        runCatching { action.actionIntent.send(this, 0, intent) }
    }

    fun dismiss(key: String) {
        runCatching { cancelNotification(key) }
    }

    fun open(key: String) {
        val pi = find(key)?.notification?.contentIntent ?: return
        runCatching {
            if (Build.VERSION.SDK_INT >= 34) {
                val opts = ActivityOptions.makeBasic().setPendingIntentBackgroundActivityStartMode(ActivityOptions.MODE_BACKGROUND_ACTIVITY_START_ALLOWED)
                pi.send(this, 0, null, null, null, null, opts.toBundle())
            } else {
                pi.send()
            }
        }
    }

    companion object {
        @Volatile
        var instance: NotifListener? = null
    }
}
