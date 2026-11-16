package app.zodyvel.zkeystore

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat

/**
 * G3 · Ekransız başlatma. Rust (headless.rs) bu durağan yöntemleri JNI ile çağırır; imzalar değiştirilmemeli.
 * Günlüğe içerik/anahtar yazılmaz.
 */
object Headless {
    @Volatile private var app: Context? = null
    @Volatile private var loaded = false

    @JvmStatic external fun nativeStart(dir: String)

    /** Tekrar çağrılması zararsız (Rust tarafı da tekil). */
    @Synchronized
    fun start(ctx: Context) {
        app = ctx.applicationContext
        // A-D2: önceki süreçten kalan onay bildirimleri (eski jetonlu düğmeler) kalkar.
        try {
            Notifier.clearStaleApprovals(ctx.applicationContext)
        } catch (e: Exception) {
        }
        val dir = Vault.coreDir(ctx) ?: return
        if (!loaded) {
            try {
                System.loadLibrary("zodyvel_phone_lib")
                loaded = true
            } catch (e: UnsatisfiedLinkError) {
                return
            }
        }
        try {
            nativeStart(dir)
        } catch (e: UnsatisfiedLinkError) {
        }
    }

    @JvmStatic fun deviceSecure(): Boolean = app?.let { Vault.deviceSecure(it) } ?: false

    @JvmStatic fun deviceName(): String = Vault.deviceName()

    @JvmStatic fun wrap(hex: String): String? = try {
        Vault.hex(Vault.wrap(Vault.unhex(hex)))
    } catch (e: Exception) {
        null
    }

    @JvmStatic fun unwrap(hex: String): String? = try {
        Vault.hex(Vault.unwrap(Vault.unhex(hex)))
    } catch (e: Exception) {
        null
    }

    @JvmStatic fun notice(kind: String, id: String, session: String, text: String, perm: Boolean) {
        val c = app ?: return
        try {
            Notifier.post(c, kind, id, session, text, perm)
        } catch (e: Exception) {
        }
    }

    @JvmStatic fun service(on: Boolean) {
        val c = app ?: return
        try {
            if (on) {
                Notifier.ensureChannels(c)
                ContextCompat.startForegroundService(c, Intent(c, BridgeService::class.java))
            } else {
                c.stopService(Intent(c, BridgeService::class.java))
                Notifier.clearAll(c)
            }
        } catch (e: Exception) {
        }
    }

    @JvmStatic fun reminders(json: String) {
        val c = app ?: return
        try {
            Reminders.sync(c, json)
        } catch (e: Exception) {
        }
    }

    @JvmStatic fun multicast(on: Boolean) {
        val c = app ?: return
        try {
            Multicast.set(c, on)
        } catch (e: Exception) {
        }
    }
}
