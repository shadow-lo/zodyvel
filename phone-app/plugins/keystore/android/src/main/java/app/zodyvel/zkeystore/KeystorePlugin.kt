package app.zodyvel.zkeystore

import android.Manifest
import android.app.Activity
import android.app.KeyguardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import android.view.WindowManager
import android.webkit.WebView
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class ChallengeArgs {
    var challenge: String = ""
}

@InvokeArg
class SignArgs {
    var msg: String = ""
    var title: String = ""
    var subtitle: String = ""
}

@InvokeArg
class PromptArgs {
    var title: String = ""
    var subtitle: String = ""
}

@InvokeArg
class DataArgs {
    var data: String = ""
}

@InvokeArg
class JsonArgs {
    var json: String = "[]"
}

@InvokeArg
class OnArgs {
    var on: Boolean = false
}

@InvokeArg
class DirArgs {
    var dir: String = ""
}

@InvokeArg
class EventsArgs {
    lateinit var channel: Channel
}

@InvokeArg
class NotifyArgs {
    var kind: String = ""
    var id: String = ""
    var session: String = ""
    var text: String = ""
    var perm: Boolean = false
}

/**
 * Zodyvel telefon: Android Keystore + BiometricPrompt.
 *  - İmza anahtarı (zv_sign / zv_sign_b, etkin ad Vault.signAlias'ta): ECDSA P-256, her kullanımda kimlik doğrulama ister (parmak izi/yüz/PIN),
 *    yeni parmak izi eklenirse geçersizleşir, mümkünse StrongBox/TEE'de, anahtar kanıtı (attestation) zinciriyle.
 *  - Sarma anahtarı (zv_wrap): AES-256-GCM, Noise özel anahtarını diskte sarılı tutar (kimlik doğrulaması istemez,
 *    arka plan hizmeti sessizce bağlanabilsin diye; eylem yetkisi imza anahtarındadır).
 *  - FLAG_SECURE: ekran görüntüsü ve son uygulamalar önizlemesi kapalı.
 * Komutlar webview'dan çağrılamaz; yalnız uygulamanın Rust kodu çağırır.
 */
@TauriPlugin
class KeystorePlugin(private val activity: Activity) : Plugin(activity) {
    private val WRAP_ALIAS = "zv_wrap"

    override fun load(webView: WebView) {
        Notifier.activityClass = activity.javaClass
        Notifier.ensureChannels(activity.applicationContext)
        // A-D2: önceki süreçten kalan, düğmeleri artık çalışmayan onay bildirimleri kalkar.
        try {
            Notifier.clearStaleApprovals(activity.applicationContext)
        } catch (e: Exception) {
        }
        handleIntent(activity.intent)
        activity.runOnUiThread {
            activity.window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
            if (Build.VERSION.SDK_INT >= 33) {
                activity.setRecentsScreenshotEnabled(false)
            }
        }
    }

    private fun hex(b: ByteArray) = b.joinToString("") { "%02x".format(it) }
    private fun unhex(s: String): ByteArray = ByteArray(s.length / 2) { s.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
    private fun ks(): KeyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    @Command
    fun device_info(invoke: Invoke) {
        val km = activity.getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
        val o = JSObject()
        o.put("secure", km.isDeviceSecure)
        o.put("name", Build.MODEL ?: "Telefon")
        invoke.resolve(o)
    }

    @Command
    fun create_signing_key(invoke: Invoke) {
        val args = invoke.parseArgs(ChallengeArgs::class.java)
        try {
            val ctx = activity.applicationContext
            val store = ks()
            // A-D6: eski anahtar yerinde kalır; yeni anahtar boştaki adla üretilir (orada yarım kalmış bir deneme varsa silinir).
            val oldAlias = Vault.signAlias(ctx)
            val newAlias = Vault.spareSignAlias(ctx)
            store.deleteEntry(newAlias)
            val challenge = unhex(args.challenge)
            val hasStrongBox = activity.packageManager.hasSystemFeature(PackageManager.FEATURE_STRONGBOX_KEYSTORE)

            fun spec(strongBox: Boolean, invalidate: Boolean): KeyGenParameterSpec {
                val b = KeyGenParameterSpec.Builder(newAlias, KeyProperties.PURPOSE_SIGN)
                    .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                    .setDigests(KeyProperties.DIGEST_SHA256)
                    .setUserAuthenticationRequired(true)
                    .setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG or KeyProperties.AUTH_DEVICE_CREDENTIAL)
                    .setInvalidatedByBiometricEnrollment(invalidate)
                    .setAttestationChallenge(challenge)
                if (strongBox) b.setIsStrongBoxBacked(true)
                return b.build()
            }

            // Sıra: (StrongBox, yeni parmak izinde geçersiz) → TEE → geçersizlik olmadan (parmak izi hiç kayıtlı değilse).
            val attempts = ArrayList<Pair<Boolean, Boolean>>()
            if (hasStrongBox) attempts.add(Pair(true, true))
            attempts.add(Pair(false, true))
            attempts.add(Pair(false, false))
            var done = false
            var strongUsed = false
            var invalidates = true
            var last: Exception? = null
            for ((sb, inv) in attempts) {
                try {
                    val gen = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore")
                    gen.initialize(spec(sb, inv))
                    gen.generateKeyPair()
                    done = true
                    strongUsed = sb
                    invalidates = inv
                    break
                } catch (e: StrongBoxUnavailableException) {
                    last = e
                } catch (e: Exception) {
                    last = e
                }
            }
            if (!done) {
                // Eski anahtar (ve eşleşme) korunur.
                store.deleteEntry(newAlias)
                invoke.reject("key_failed")
                return
            }

            // Açık anahtar ve kanıt zinciri yeni addan okunur; ikisi de alınamazsa yer değiştirilmez.
            val cert = store.getCertificate(newAlias)
            val pub = cert.publicKey as ECPublicKey
            fun pad(b: ByteArray): ByteArray {
                val t = if (b.size > 32) b.copyOfRange(b.size - 32, b.size) else b
                return ByteArray(32 - t.size) + t
            }
            val sec1 = byteArrayOf(0x04) + pad(pub.w.affineX.toByteArray()) + pad(pub.w.affineY.toByteArray())
            val att = JSArray()
            store.getCertificateChain(newAlias)?.forEach { att.put(hex(it.encoded)) }
            if (!Vault.setSignAlias(ctx, newAlias)) {
                store.deleteEntry(newAlias)
                invoke.reject("key_failed")
                return
            }
            // Yer değiştirme tamam: eski anahtar artık silinebilir.
            try {
                store.deleteEntry(oldAlias)
            } catch (e: Exception) {
            }
            val o = JSObject()
            o.put("apub", hex(sec1))
            o.put("attestation", att)
            o.put("strongbox", strongUsed)
            o.put("invalidates", invalidates)
            invoke.resolve(o)
        } catch (e: Exception) {
            invoke.reject("key_failed")
        }
    }

    private fun prompt(title: String, subtitle: String, crypto: BiometricPrompt.CryptoObject?, invoke: Invoke, onOk: (BiometricPrompt.AuthenticationResult) -> Unit) {
        val act = activity as? FragmentActivity
        if (act == null) {
            invoke.reject("no_activity")
            return
        }
        act.runOnUiThread {
            val executor = ContextCompat.getMainExecutor(act)
            val cb = object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    try {
                        onOk(result)
                    } catch (e: Exception) {
                        invoke.reject("auth_failed")
                    }
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    val cancelled = errorCode == BiometricPrompt.ERROR_USER_CANCELED ||
                        errorCode == BiometricPrompt.ERROR_NEGATIVE_BUTTON ||
                        errorCode == BiometricPrompt.ERROR_CANCELED
                    invoke.reject(if (cancelled) "canceled" else "auth_error")
                }
            }
            val bp = BiometricPrompt(act, executor, cb)
            val info = BiometricPrompt.PromptInfo.Builder()
                .setTitle(title)
                .setSubtitle(subtitle)
                .setAllowedAuthenticators(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
                .build()
            if (crypto != null) bp.authenticate(info, crypto) else bp.authenticate(info)
        }
    }

    @Command
    fun sign(invoke: Invoke) {
        val a = invoke.parseArgs(SignArgs::class.java)
        try {
            val key = ks().getKey(Vault.signAlias(activity.applicationContext), null) as? PrivateKey
            if (key == null) {
                invoke.reject("no_key")
                return
            }
            val sig = Signature.getInstance("SHA256withECDSA")
            try {
                sig.initSign(key)
            } catch (e: KeyPermanentlyInvalidatedException) {
                invoke.reject("key_invalidated")
                return
            }
            val msg = unhex(a.msg)
            prompt(a.title, a.subtitle, BiometricPrompt.CryptoObject(sig), invoke) { r ->
                val s = r.cryptoObject!!.signature!!
                s.update(msg)
                val o = JSObject()
                o.put("sig", hex(s.sign()))
                invoke.resolve(o)
            }
        } catch (e: Exception) {
            invoke.reject("sign_failed")
        }
    }

    @Command
    fun authenticate(invoke: Invoke) {
        val a = invoke.parseArgs(PromptArgs::class.java)
        prompt(a.title, a.subtitle, null, invoke) { invoke.resolve(JSObject()) }
    }

    @Command
    fun wrap(invoke: Invoke) {
        val a = invoke.parseArgs(DataArgs::class.java)
        try {
            val o = JSObject()
            o.put("data", hex(Vault.wrap(unhex(a.data))))
            invoke.resolve(o)
        } catch (e: Exception) {
            invoke.reject("wrap_failed")
        }
    }

    @Command
    fun unwrap(invoke: Invoke) {
        val a = invoke.parseArgs(DataArgs::class.java)
        try {
            val raw = unhex(a.data)
            if (raw.size < 13) {
                invoke.reject("unwrap_failed")
                return
            }
            val o = JSObject()
            o.put("data", hex(Vault.unwrap(raw)))
            invoke.resolve(o)
        } catch (e: Exception) {
            invoke.reject("unwrap_failed")
        }
    }

    @Command
    fun delete_keys(invoke: Invoke) {
        try {
            val s = ks()
            s.deleteEntry(Vault.SIGN_ALIAS)
            s.deleteEntry(Vault.SIGN_ALIAS_ALT)
            s.deleteEntry(WRAP_ALIAS)
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("delete_failed")
        }
    }

    // ---- T4-b: ön plan hizmeti, bildirimler, bildirim eylemleri ----

    private var events: Channel? = null
    private val pendingEvents = ArrayList<JSObject>()

    private fun emit(o: JSObject) {
        synchronized(pendingEvents) {
            val ch = events
            if (ch == null) {
                if (pendingEvents.size < 8) pendingEvents.add(o)
            } else {
                ch.send(o)
            }
        }
    }

    /** Bildirimden gelen İzin ver / Reddet: yalnız bu sürecin jetonunu taşıyan intent kabul edilir. Doğrulama ve imza Rust tarafında yapılır. */
    private fun handleIntent(intent: Intent?) {
        if (intent == null || intent.action != Notifier.ACTION_DECIDE) return
        val ok = intent.getStringExtra(Notifier.X_NONCE) == Notifier.nonce
        val id = intent.getStringExtra(Notifier.X_ID)
        val allow = intent.getBooleanExtra(Notifier.X_ALLOW, false)
        // Yeniden teslimi (ekran döndürme vb.) önlemek için tüket.
        intent.action = null
        intent.data = null
        intent.removeExtra(Notifier.X_NONCE)
        intent.removeExtra(Notifier.X_ID)
        if (!ok || id.isNullOrEmpty()) return
        val o = JSObject()
        o.put("k", "decide")
        o.put("id", id)
        o.put("allow", allow)
        emit(o)
    }

    override fun onNewIntent(intent: Intent) {
        handleIntent(intent)
    }

    @Command
    fun set_events(invoke: Invoke) {
        val a = invoke.parseArgs(EventsArgs::class.java)
        synchronized(pendingEvents) {
            events = a.channel
            pendingEvents.forEach { a.channel.send(it) }
            pendingEvents.clear()
        }
        invoke.resolve(JSObject())
    }

    @Command
    fun service_start(invoke: Invoke) {
        try {
            val ctx = activity.applicationContext
            Notifier.ensureChannels(ctx)
            if (Build.VERSION.SDK_INT >= 33 &&
                ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            ) {
                activity.runOnUiThread { activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 4102) }
            }
            ContextCompat.startForegroundService(ctx, Intent(ctx, BridgeService::class.java))
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("service_failed")
        }
    }

    @Command
    fun service_stop(invoke: Invoke) {
        try {
            val ctx = activity.applicationContext
            ctx.stopService(Intent(ctx, BridgeService::class.java))
            Notifier.clearAll(ctx)
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("service_failed")
        }
    }

    @Command
    fun post_notice(invoke: Invoke) {
        val a = invoke.parseArgs(NotifyArgs::class.java)
        try {
            val ctx = activity.applicationContext
            Notifier.post(ctx, a.kind, a.id, a.session, a.text, a.perm)
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("notify_failed")
        }
    }

    // ---- G3: Ajanda hatırlatıcıları, mDNS çok yöne yayın kilidi, çekirdek klasörü ----

    @Command
    fun reminders_sync(invoke: Invoke) {
        val a = invoke.parseArgs(JsonArgs::class.java)
        try {
            Reminders.sync(activity.applicationContext, a.json)
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("reminders_failed")
        }
    }

    @Command
    fun multicast(invoke: Invoke) {
        val a = invoke.parseArgs(OnArgs::class.java)
        try {
            Multicast.set(activity.applicationContext, a.on)
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("multicast_failed")
        }
    }

    @Command
    fun set_core_dir(invoke: Invoke) {
        val a = invoke.parseArgs(DirArgs::class.java)
        try {
            Vault.prefs(activity.applicationContext).edit().putString("core_dir", a.dir).apply()
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("dir_failed")
        }
    }

    /** Pil optimizasyonundan çıkma izni sorar (Xiaomi/Samsung ön plan hizmetini kapatabiliyor). Zaten muafsa hiçbir şey açmaz. */
    @Command
    fun battery_exempt(invoke: Invoke) {
        try {
            val ctx = activity.applicationContext
            val pm = ctx.getSystemService(Context.POWER_SERVICE) as PowerManager
            val o = JSObject()
            if (pm.isIgnoringBatteryOptimizations(ctx.packageName)) {
                o.put("exempt", true)
            } else {
                o.put("exempt", false)
                val i = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:" + ctx.packageName))
                activity.runOnUiThread { activity.startActivity(i) }
            }
            invoke.resolve(o)
        } catch (e: Exception) {
            invoke.reject("battery_failed")
        }
    }
}
