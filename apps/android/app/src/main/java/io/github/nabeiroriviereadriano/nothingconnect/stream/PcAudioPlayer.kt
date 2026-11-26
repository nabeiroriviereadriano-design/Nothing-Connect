package io.github.nabeiroriviereadriano.nothingconnect.stream

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.TimeUnit

/** Lecture du son du PC (PCM 16 bits, 48 kHz, stéréo) avec une latence plafonnée (~250 ms). */
class PcAudioPlayer {
    private val queue = LinkedBlockingDeque<ByteArray>()
    @Volatile private var running = true
    private val track: AudioTrack
    private val thread: Thread

    init {
        val min = AudioTrack.getMinBufferSize(48_000, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT)
        track = AudioTrack.Builder()
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setAudioFormat(AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(48_000).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build())
            .setBufferSizeInBytes(maxOf(min * 2, 1920 * 6))
            .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
            .setTransferMode(AudioTrack.MODE_STREAM)
            .build()
        track.play()
        thread = Thread({
            while (running) {
                val chunk = queue.poll(200, TimeUnit.MILLISECONDS) ?: continue
                track.write(chunk, 0, chunk.size)
            }
        }, "nc-pc-audio").apply { start() }
    }

    fun push(pcm: ByteArray) {
        queue.offer(pcm)
        // 25 paquets de 10 ms : au-delà on jette le plus ancien plutôt que d'accumuler du retard.
        while (queue.size > 25) queue.pollFirst()
    }

    fun stop() {
        running = false
        thread.join(500)
        runCatching { track.stop() }
        track.release()
    }
}
