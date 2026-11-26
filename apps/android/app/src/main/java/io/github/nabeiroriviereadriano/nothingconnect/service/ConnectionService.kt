package io.github.nabeiroriviereadriano.nothingconnect.service

import android.app.NotificationManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.Uri
import android.net.wifi.WifiManager
import android.app.PendingIntent
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleService
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.core.Notifs
import io.github.nabeiroriviereadriano.nothingconnect.notif.MediaWatcher
import io.github.nabeiroriviereadriano.nothingconnect.notif.NotifListener
import io.github.nabeiroriviereadriano.nothingconnect.stream.Streams
import org.json.JSONObject

/**
 * Service de premier plan (type connectedDevice) qui maintient la connexion aux PC associés.
 * Android peut malgré tout le limiter (économie d'énergie, surcouches constructeur) :
 * l'écran Autorisations guide l'utilisateur pour exclure l'appli de l'optimisation batterie.
 */
class ConnectionService : LifecycleService() {
    private var multicast: WifiManager.MulticastLock? = null
    private var status: StatusReporter? = null
    private var commands: Commands? = null
    private var received: ReceivedFiles? = null
    private val listener: (JSONObject) -> Unit = { e -> onCoreEvent(e) }
    private val netCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) = Core.node.networkChanged()
        override fun onLost(network: Network) = Core.node.networkChanged()
    }

    override fun onCreate() {
        super.onCreate()
        ServiceCompat.startForeground(
            this, Notifs.ID_SERVICE, Notifs.service(this, "Recherche du PC…"),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE,
        )
        // Nécessaire à la découverte mDNS du PC sur le Wi-Fi.
        multicast = (applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
            .createMulticastLock("nothing-connect").apply { setReferenceCounted(false); acquire() }
        getSystemService(ConnectivityManager::class.java).registerNetworkCallback(
            NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build(), netCallback,
        )
        status = StatusReporter(this).also { it.start() }
        commands = Commands(this)
        received = ReceivedFiles(this)
        Core.subscribe(listener)
        Core.start()
        MediaWatcher.ensure(this)
        updateText()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        return START_STICKY
    }

    override fun onDestroy() {
        Core.unsubscribe(listener)
        Streams.stopPcAudio()
        status?.stop()
        commands?.stopRing()
        runCatching { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(netCallback) }
        multicast?.release()
        super.onDestroy()
    }

    private fun updateText() {
        val c = Core.connected.value
        val text = when {
            c.isEmpty() -> "En attente d'un PC associé sur le Wi-Fi"
            else -> "Connecté à " + Core.devices.value.filter { it.id in c.keys }.joinToString { it.name }
        }
        getSystemService(NotificationManager::class.java).notify(Notifs.ID_SERVICE, Notifs.service(this, text))
    }

    private fun onCoreEvent(e: JSONObject) {
        when (e.getString("type")) {
            "peer_connected" -> {
                updateText()
                val peer = e.getString("peer_id")
                status?.sendNow(peer)
                NotifListener.instance?.sendAppList()
                NotifListener.instance?.resendActive(peer)
                MediaWatcher.publish()
            }
            "peer_disconnected", "device_removed" -> {
                updateText()
                Streams.onPeerGone(e.getString("peer_id"))
            }
            "request" -> {
                val peer = e.getString("peer_id")
                val msg = e.getJSONObject("message")
                if (!Streams.handle(peer, msg)) commands?.handle(peer, msg)
            }
            "clipboard" -> if (Core.prefs.receiveClipboard) {
                // Écrire dans le presse-papiers est permis en arrière-plan.
                getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("NOTHING CONNECT", e.getString("content")))
            }
            "transfer_incoming" -> {
                val i = Core.incoming.value.firstOrNull { it.id == e.getString("id") } ?: return
                val name = Core.devices.value.firstOrNull { it.id == i.peerId }?.name ?: "Le PC"
                Notifs.incoming(this, i, name)
            }
            "transfer_progress", "transfer_finished" -> {
                Core.transfers.value[e.getString("id")]?.let { Notifs.transfer(this, it) }
                if (e.getString("type") == "transfer_finished") Notifs.clearIncoming(this, e.getString("id"))
            }
            "transfer_file_done" -> received?.onFileDone(e)
            "share" -> onShare(e.getString("kind"), e.getString("content"))
        }
    }

    private fun onShare(kind: String, content: String) {
        if (kind == "link") {
            val pi = PendingIntent.getActivity(
                this, content.hashCode(), Intent(Intent.ACTION_VIEW, Uri.parse(content)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                PendingIntent.FLAG_IMMUTABLE,
            )
            Notifs.simple(this, "Lien reçu du PC", content, pi)
        } else {
            // Écrire dans le presse-papiers est autorisé en arrière-plan (seule la lecture est restreinte).
            getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("NOTHING CONNECT", content))
            Notifs.simple(this, "Texte reçu du PC (copié)", content)
        }
    }

    companion object {
        fun start(ctx: Context) {
            ContextCompat.startForegroundService(ctx, Intent(ctx, ConnectionService::class.java))
        }

        fun stop(ctx: Context) {
            ctx.stopService(Intent(ctx, ConnectionService::class.java))
        }
    }
}
