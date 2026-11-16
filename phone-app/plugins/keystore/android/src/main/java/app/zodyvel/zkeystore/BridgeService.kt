package app.zodyvel.zkeystore

import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder

/**
 * Ön plan hizmeti: yalnız süreci canlı tutar; LAN köprü bağlantısı Rust tarafındaki canlı iş parçacığındadır
 * (zodyvel-phone-core::live). Hizmet yoksa o iş parçacığı da çalışmaz (uygulama durur).
 * Süreç öldürülürse hizmet kendiliğinden geri gelmez (START_NOT_STICKY): içinde Rust çalışmayan bir "bağlı" bildirimi
 * yanıltıcı olurdu; kullanıcı uygulamayı açınca yeniden başlar.
 * A-O3: Wi-Fi kilidi kaldırıldı. FULL_LOW_LATENCY yalnız ön planda ve ekran açıkken etkili, FULL_HIGH_PERF API 34'ten
 * beri etkisiz; kilit hiçbir işe yaramadan tutuluyordu.
 */
class BridgeService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent == null) {
            stopSelf()
            return START_NOT_STICKY
        }
        Notifier.ensureChannels(this)
        val n = Notifier.serviceNotification(this, Notifier.serviceText)
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(Notifier.SERVICE_ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(Notifier.SERVICE_ID, n)
        }
        if (intent.getBooleanExtra("headless", false)) {
            Headless.start(applicationContext)
        }
        return START_NOT_STICKY
    }
}
