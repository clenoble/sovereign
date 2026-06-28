package com.sovereign.ge

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  private var multicastLock: WifiManager.MulticastLock? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // android-flagsecure-missing (v0.0.8 audit): mark the window secure so the
    // post-login decrypted canvas, chat, and PII surfaces are excluded from
    // screenshots and the Recents thumbnail cache. Set unconditionally — the
    // login gate itself is non-sensitive, but the app transitions into
    // decrypted content within the same Activity, so a single flag is simplest
    // and closes the recurring leak for a privacy-first app.
    window.setFlags(
      WindowManager.LayoutParams.FLAG_SECURE,
      WindowManager.LayoutParams.FLAG_SECURE,
    )

    // Hold a Wi-Fi multicast lock so libp2p's mDNS can RECEIVE peer
    // announcements for P2P LAN sync. Android filters multicast packets to
    // untrusted apps unless this lock is held (paired with the
    // CHANGE_WIFI_MULTICAST_STATE permission in the manifest). Without it,
    // the phone can send mDNS queries but never sees the peer's address,
    // so sync dials fail with DialFailure.
    val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
    multicastLock = wifi.createMulticastLock("sovereign-mdns").apply {
      setReferenceCounted(false)
      acquire()
    }
  }

  override fun onDestroy() {
    multicastLock?.let { if (it.isHeld) it.release() }
    super.onDestroy()
  }
}
