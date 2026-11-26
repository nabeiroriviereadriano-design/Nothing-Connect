package io.github.nabeiroriviereadriano.nothingconnect.core

import android.content.Context

/** Préférences propres au téléphone (le reste est dans la base du cœur Rust). */
class Prefs(context: Context) {
    private val p = context.getSharedPreferences("nc_prefs", Context.MODE_PRIVATE)

    var serviceEnabled: Boolean
        get() = p.getBoolean("service_enabled", true)
        set(v) = p.edit().putBoolean("service_enabled", v).apply()

    var shareNotifications: Boolean
        get() = p.getBoolean("share_notifications", true)
        set(v) = p.edit().putBoolean("share_notifications", v).apply()

    /** Copier sur le téléphone le texte copié sur le PC (si le PC l'envoie). */
    var receiveClipboard: Boolean
        get() = p.getBoolean("receive_clipboard", true)
        set(v) = p.edit().putBoolean("receive_clipboard", v).apply()

    /** Applications dont les notifications ne sont pas transmises. */
    fun isAppBlocked(pkg: String): Boolean = p.getStringSet("blocked_apps", DEFAULT_BLOCKED)!!.contains(pkg)

    fun setAppBlocked(pkg: String, blocked: Boolean) {
        val s = p.getStringSet("blocked_apps", DEFAULT_BLOCKED)!!.toMutableSet()
        if (blocked) s.add(pkg) else s.remove(pkg)
        p.edit().putStringSet("blocked_apps", s).apply()
    }

    companion object {
        /** Par défaut, rien n'est bloqué ; l'utilisateur choisit depuis le PC ou le téléphone. */
        val DEFAULT_BLOCKED: Set<String> = emptySet()
    }
}
