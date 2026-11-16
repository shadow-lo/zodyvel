package app.zodyvel.zkeystore

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.graphics.drawable.Icon
import android.net.Uri
import android.os.Build
import java.security.SecureRandom
import java.util.concurrent.atomic.AtomicInteger

/**
 * Zodyvel telefon bildirimleri. Kurallar (Plan 3 · 2.5/7):
 *  - Kilit ekranında içerik YOK: her bildirimin gizli sürümü (publicVersion) yalnız "Zodyvel: 1 onay bekliyor" gibi genel metindir.
 *  - Bildirime istem, kod, komut çıktısı ya da adım metni girmez; yalnız oturum adı ve olay türü.
 *  - İzin ver / Reddet eylemi uygulamayı açar ve BiometricPrompt + Keystore imzası ister (imza Rust tarafında, sign);
 *    eylem bildirimden doğrudan işlenmez. Android 12+'da cihaz kilidi açıkken çalışır (setAuthenticationRequired).
 */
object Notifier {
    const val CH_SERVICE = "zv_service"
    const val CH_ALERT = "zv_alert"
    const val CH_INFO = "zv_info"
    const val SERVICE_ID = 4101
    const val ACTION_DECIDE = "app.zodyvel.phone.DECIDE"
    const val X_NONCE = "zv_nonce"
    const val X_ID = "zv_id"
    const val X_ALLOW = "zv_allow"

    /** Her süreçte yeni rastgele jeton: yalnız bizim PendingIntent'lerimiz taşır; başka uygulamanın yolladığı sahte intent reddedilir. */
    val nonce: String by lazy {
        val b = ByteArray(16)
        SecureRandom().nextBytes(b)
        b.joinToString("") { "%02x".format(it) }
    }

    @Volatile var activityClass: Class<*>? = null
    @Volatile var serviceText: String = "Bağlanıyor"
    private val counter = AtomicInteger(5000)
    private val approvals = LinkedHashSet<String>()
    private var staleCleared = false

    private fun nm(ctx: Context) = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    fun ensureChannels(ctx: Context) {
        val m = nm(ctx)
        m.createNotificationChannel(NotificationChannel(CH_SERVICE, "Bağlantı", NotificationManager.IMPORTANCE_LOW).apply {
            description = "Bilgisayara bağlı kalmak için sürekli bildirim"
            lockscreenVisibility = Notification.VISIBILITY_PRIVATE
            setShowBadge(false)
        })
        m.createNotificationChannel(NotificationChannel(CH_ALERT, "Onay istekleri", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "AI aracı onay beklediğinde"
            lockscreenVisibility = Notification.VISIBILITY_PRIVATE
        })
        m.createNotificationChannel(NotificationChannel(CH_INFO, "Görev durumu", NotificationManager.IMPORTANCE_DEFAULT).apply {
            description = "Görev bittiğinde ya da hata verdiğinde"
            lockscreenVisibility = Notification.VISIBILITY_PRIVATE
        })
    }

    private fun builder(ctx: Context, channel: String) =
        Notification.Builder(ctx, channel).setSmallIcon(Icon.createWithResource(ctx, R.drawable.ic_zv_notif))

    /** Kilit ekranında görünecek içeriksiz sürüm. */
    private fun publicVersion(ctx: Context, channel: String, text: String): Notification =
        builder(ctx, channel).setContentTitle("Zodyvel").setContentText(text).setVisibility(Notification.VISIBILITY_PUBLIC).build()

    private fun launchIntent(ctx: Context): Intent {
        val cls = activityClass
        return if (cls != null) Intent(ctx, cls)
        else ctx.packageManager.getLaunchIntentForPackage(ctx.packageName) ?: Intent()
    }

    private fun openApp(ctx: Context): PendingIntent {
        val i = launchIntent(ctx).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        return PendingIntent.getActivity(ctx, 1, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    fun serviceNotification(ctx: Context, text: String): Notification {
        serviceText = text
        return builder(ctx, CH_SERVICE)
            .setContentTitle("Zodyvel")
            .setContentText(text)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setContentIntent(openApp(ctx))
            .setVisibility(Notification.VISIBILITY_PRIVATE)
            .setPublicVersion(publicVersion(ctx, CH_SERVICE, "Çalışıyor"))
            .build()
    }

    fun updateService(ctx: Context, text: String) {
        if (text == serviceText) return
        serviceText = text
        nm(ctx).notify(SERVICE_ID, serviceNotification(ctx, text))
    }

    private fun decideAction(ctx: Context, id: String, allow: Boolean, label: String): Notification.Action {
        // A-D1: her onay/düğme için ayrı veri URI'si; requestCode hash'i çakışsa da PendingIntent'ler karışmaz
        // (FLAG_UPDATE_CURRENT başka bir onayın kimliğini ezemez).
        val i = launchIntent(ctx)
            .setAction(ACTION_DECIDE)
            .setData(Uri.parse("zv://decide/${Uri.encode(id)}/$allow"))
            .putExtra(X_NONCE, nonce)
            .putExtra(X_ID, id)
            .putExtra(X_ALLOW, allow)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        val code = id.hashCode() * 2 + (if (allow) 1 else 0)
        val pi = PendingIntent.getActivity(ctx, code, i, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val b = Notification.Action.Builder(Icon.createWithResource(ctx, R.drawable.ic_zv_notif), label, pi)
        if (Build.VERSION.SDK_INT >= 31) b.setAuthenticationRequired(true)
        return b.build()
    }

    @Synchronized
    fun approval(ctx: Context, id: String, session: String, withActions: Boolean) {
        approvals.add(id)
        val n = approvals.size
        val b = builder(ctx, CH_ALERT)
            .setContentTitle("Onay bekliyor")
            .setContentText(session)
            .setCategory(Notification.CATEGORY_STATUS)
            .setAutoCancel(false)
            .setContentIntent(openApp(ctx))
            .setVisibility(Notification.VISIBILITY_PRIVATE)
            .setPublicVersion(publicVersion(ctx, CH_ALERT, if (n == 1) "1 onay bekliyor" else "$n onay bekliyor"))
        if (withActions) {
            b.addAction(decideAction(ctx, id, true, "İzin ver"))
            b.addAction(decideAction(ctx, id, false, "Reddet"))
        }
        nm(ctx).notify(id, 1, b.build())
    }

    /**
     * A-D2: Önceki süreçten kalan onay bildirimlerini kaldırır. Onların düğmeleri eski jetonu taşıdığı için sessizce
     * reddedilirdi; bekleyen onaylar Rust yeniden bağlanınca yeni jetonla tekrar gelir. Bu süreçte gönderilenlere dokunulmaz.
     */
    @Synchronized
    fun clearStaleApprovals(ctx: Context) {
        // Süreç başına bir kez yeter: sonrakiler zaten bu sürecin jetonunu taşır.
        if (staleCleared) return
        staleCleared = true
        val m = nm(ctx)
        for (sbn in m.activeNotifications) {
            if (sbn.notification.channelId != CH_ALERT) continue
            val tag = sbn.tag
            if (tag != null && approvals.contains(tag)) continue
            m.cancel(tag, sbn.id)
        }
    }

    @Synchronized
    fun cancelApproval(ctx: Context, id: String) {
        approvals.remove(id)
        nm(ctx).cancel(id, 1)
    }

    fun finished(ctx: Context, session: String, ok: Boolean) {
        val b = builder(ctx, CH_INFO)
            .setContentTitle(if (ok) "Görev bitti" else "Görev hata verdi")
            .setContentText(session)
            .setAutoCancel(true)
            .setContentIntent(openApp(ctx))
            .setVisibility(Notification.VISIBILITY_PRIVATE)
            .setPublicVersion(publicVersion(ctx, CH_INFO, if (ok) "Bir görev bitti" else "Bir görev hata verdi"))
        nm(ctx).notify(counter.incrementAndGet(), b.build())
    }

    fun info(ctx: Context, text: String) {
        val b = builder(ctx, CH_INFO)
            .setContentTitle("Zodyvel")
            .setContentText(text)
            .setAutoCancel(true)
            .setTimeoutAfter(60_000)
            .setContentIntent(openApp(ctx))
            .setVisibility(Notification.VISIBILITY_PRIVATE)
            .setPublicVersion(publicVersion(ctx, CH_INFO, "Zodyvel: yeni bildirim"))
        nm(ctx).notify(counter.incrementAndGet(), b.build())
    }

    /** Eklenti ve ekransız mod ortak: Rust'tan gelen olay türüne göre bildirim. */
    fun post(ctx: Context, kind: String, id: String, session: String, text: String, perm: Boolean) {
        when (kind) {
            "service" -> updateService(ctx, text)
            "approval" -> approval(ctx, id, session, perm)
            "gone" -> cancelApproval(ctx, id)
            "finished" -> finished(ctx, session, true)
            "failed" -> finished(ctx, session, false)
            "info" -> info(ctx, text)
        }
    }

    /** Hizmet durunca ya da eşleşme silinince: bu uygulamanın tüm bildirimleri (onay istekleri dahil) kalkar. */
    @Synchronized
    fun clearAll(ctx: Context) {
        approvals.clear()
        nm(ctx).cancelAll()
    }
}
