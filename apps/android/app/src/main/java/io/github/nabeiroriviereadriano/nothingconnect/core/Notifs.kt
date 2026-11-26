package io.github.nabeiroriviereadriano.nothingconnect.core

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import io.github.nabeiroriviereadriano.nothingconnect.R
import io.github.nabeiroriviereadriano.nothingconnect.service.ActionReceiver
import io.github.nabeiroriviereadriano.nothingconnect.ui.MainActivity

object Notifs {
    const val CH_SERVICE = "service"
    const val CH_TRANSFER = "transfer"
    const val CH_INCOMING = "incoming"
    const val CH_STREAM = "stream"
    const val ID_SERVICE = 1
    const val ID_MIRROR = 2

    fun createChannels(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CH_SERVICE, "Connexion au PC", NotificationManager.IMPORTANCE_MIN).apply {
            description = "Indique que NOTHING CONNECT maintient la connexion en arrière-plan."
            setShowBadge(false)
        })
        nm.createNotificationChannel(NotificationChannel(CH_TRANSFER, "Transferts", NotificationManager.IMPORTANCE_LOW).apply {
            description = "Progression des envois et réceptions de fichiers."
        })
        nm.createNotificationChannel(NotificationChannel(CH_INCOMING, "Demandes et réceptions", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Fichiers à accepter, liens et textes reçus du PC."
        })
        nm.createNotificationChannel(NotificationChannel(CH_STREAM, "Écran et son partagés", NotificationManager.IMPORTANCE_LOW).apply {
            description = "Indique que l'écran ou le son du téléphone est diffusé vers le PC."
        })
    }

    private fun openApp(ctx: Context): PendingIntent = PendingIntent.getActivity(
        ctx, 0, Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    fun action(ctx: Context, action: String, extra: String, req: Int): PendingIntent = PendingIntent.getBroadcast(
        ctx, req, Intent(ctx, ActionReceiver::class.java).setAction(action).putExtra(ActionReceiver.EXTRA, extra),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    fun service(ctx: Context, text: String): Notification =
        NotificationCompat.Builder(ctx, CH_SERVICE)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("NOTHING CONNECT")
            .setContentText(text)
            .setOngoing(true)
            .setContentIntent(openApp(ctx))
            .addAction(0, "Arrêter", action(ctx, ActionReceiver.STOP_SERVICE, "", 1))
            .setForegroundServiceBehavior(NotificationCompat.FOREGROUND_SERVICE_IMMEDIATE)
            .build()

    fun transfer(ctx: Context, t: TransferUi) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        val id = 1000 + (t.id.hashCode() and 0xffff)
        val finished = t.state in setOf("completed", "failed", "cancelled", "rejected")
        val title = when {
            t.state == "completed" -> if (t.outgoing) "Envoi terminé" else "Fichier reçu"
            t.state == "failed" -> "Transfert échoué"
            t.state == "cancelled" || t.state == "rejected" -> "Transfert annulé"
            t.state == "paused" -> "Transfert en pause"
            t.state == "interrupted" -> "Transfert interrompu, reprise à la reconnexion"
            else -> if (t.outgoing) "Envoi vers le PC" else "Réception depuis le PC"
        }
        val pct = if (t.total > 0) ((t.done * 100) / t.total).toInt() else 0
        val b = NotificationCompat.Builder(ctx, CH_TRANSFER)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(title)
            .setContentText(t.error ?: t.file ?: "")
            .setOnlyAlertOnce(true)
            .setContentIntent(openApp(ctx))
            .setOngoing(!finished)
        if (!finished) {
            b.setProgress(100, pct, false)
            if (t.state == "running") b.addAction(0, "Pause", action(ctx, ActionReceiver.PAUSE, t.id, id))
            if (t.state == "paused") b.addAction(0, "Reprendre", action(ctx, ActionReceiver.RESUME, t.id, id))
            b.addAction(0, "Annuler", action(ctx, ActionReceiver.CANCEL, t.id, id + 1))
        } else {
            b.setAutoCancel(true)
        }
        runCatching { nm.notify(id, b.build()) }
    }

    fun incoming(ctx: Context, i: IncomingUi, pcName: String) {
        val id = 2000 + (i.id.hashCode() and 0xffff)
        val what = if (i.files.size == 1) i.files[0] else "${i.files.size} fichiers"
        val n = NotificationCompat.Builder(ctx, CH_INCOMING)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("$pcName veut vous envoyer $what")
            .setContentText("Appuyez sur Accepter pour recevoir.")
            .setContentIntent(openApp(ctx))
            .setAutoCancel(true)
            .addAction(0, "Accepter", action(ctx, ActionReceiver.ACCEPT, i.id, id))
            .addAction(0, "Refuser", action(ctx, ActionReceiver.REJECT, i.id, id + 1))
            .build()
        runCatching { ctx.getSystemService(NotificationManager::class.java).notify(id, n) }
    }

    fun clearIncoming(ctx: Context, transferId: String) {
        ctx.getSystemService(NotificationManager::class.java).cancel(2000 + (transferId.hashCode() and 0xffff))
    }

    fun simple(ctx: Context, title: String, text: String, intent: PendingIntent? = null) {
        val n = NotificationCompat.Builder(ctx, CH_INCOMING)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(title)
            .setContentText(text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text))
            .setAutoCancel(true)
            .setContentIntent(intent ?: openApp(ctx))
            .build()
        runCatching { ctx.getSystemService(NotificationManager::class.java).notify((System.currentTimeMillis() % 100000).toInt() + 5000, n) }
    }
}
