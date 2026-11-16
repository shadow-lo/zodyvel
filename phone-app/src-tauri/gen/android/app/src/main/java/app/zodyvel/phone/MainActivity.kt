package app.zodyvel.phone

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // G4: ekran görüntüsü ve "son uygulamalar" önizlemesi ilk kareden itibaren kapalı (eklenti yüklenmeden önce de).
    window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
