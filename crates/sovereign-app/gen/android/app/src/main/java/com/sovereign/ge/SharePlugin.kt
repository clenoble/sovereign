// ── Android Share-Sheet Receiver — scaffold ──────────────────────────────────
//
// Drop this file into the Tauri-generated Android project once Phase 0
// (cargo tauri android init) has been run. Paths:
//
//   gen/android/app/src/main/java/com/sovereign/ge/SharePlugin.kt  ← this file
//   gen/android/app/src/main/AndroidManifest.xml                  ← add intent-filter below
//
// In MainActivity.kt, add `.plugin(SharePlugin())` to the plugin chain.
//
// ─────────────────────────────────────────────────────────────────────────────
//
// AndroidManifest.xml — add inside the <activity> element for MainActivity:
//
//   <intent-filter>
//     <action android:name="android.intent.action.SEND" />
//     <category android:name="android.intent.category.DEFAULT" />
//     <data android:mimeType="text/plain" />
//   </intent-filter>
//   <intent-filter>
//     <action android:name="android.intent.action.SEND" />
//     <category android:name="android.intent.category.DEFAULT" />
//     <data android:mimeType="text/uri-list" />
//   </intent-filter>
//
// ─────────────────────────────────────────────────────────────────────────────

package com.sovereign.ge

import android.content.Intent
import android.os.Bundle
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@TauriPlugin
class SharePlugin(private val activity: android.app.Activity) : Plugin(activity) {

    // Queued intent received before the webview finished loading
    private var pendingIntent: Intent? = null

    /** Called by MainActivity.onNewIntent — forward a fresh share intent. */
    override fun onNewIntent(intent: Intent) {
        if (isWebViewReady()) {
            forwardShare(intent)
        } else {
            pendingIntent = intent
        }
    }

    /** Flush any intent that arrived before the webview was ready. */
    override fun load(webView: android.webkit.WebView) {
        super.load(webView)
        pendingIntent?.let { forwardShare(it) }
        pendingIntent = null
    }

    // ── Internal ─────────────────────────────────────────────────────────────

    private fun isWebViewReady(): Boolean = true // Tauri guarantees load() ran first

    private fun forwardShare(intent: Intent) {
        when (intent.action) {
            Intent.ACTION_SEND -> {
                val mimeType = intent.type ?: return
                val payload = JSObject()
                when {
                    mimeType == "text/plain" -> {
                        val text = intent.getStringExtra(Intent.EXTRA_TEXT) ?: ""
                        val subject = intent.getStringExtra(Intent.EXTRA_SUBJECT)
                        // Heuristic: URLs are text/plain with a URL-shaped extra
                        if (text.startsWith("http://") || text.startsWith("https://")) {
                            payload.put("content_type", "url")
                            payload.put("url", text)
                            if (subject != null) payload.put("title", subject)
                        } else {
                            payload.put("content_type", "text")
                            payload.put("text", text)
                            if (subject != null) payload.put("title", subject)
                        }
                    }
                    else -> return // unsupported MIME type — ignore
                }
                trigger("share-received", payload)
            }
        }
    }
}
