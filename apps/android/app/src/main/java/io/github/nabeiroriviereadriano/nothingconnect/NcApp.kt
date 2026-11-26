package io.github.nabeiroriviereadriano.nothingconnect

import android.app.Application
import io.github.nabeiroriviereadriano.nothingconnect.core.Core
import io.github.nabeiroriviereadriano.nothingconnect.core.Notifs

class NcApp : Application() {
    override fun onCreate() {
        super.onCreate()
        Notifs.createChannels(this)
        Core.init(this)
    }
}
