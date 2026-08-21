package net.buwang.camerars;

import android.annotation.SuppressLint;
import android.os.Bundle;
import android.view.View;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;

import androidx.activity.OnBackPressedCallback;
import androidx.appcompat.app.AppCompatActivity;

/**
 * camera_rs Android 客户端：WebView 壳
 * 默认加载内置摄像头服务地址，可在输入框切换服务器。
 */
public class MainActivity extends AppCompatActivity {

    private static final String DEFAULT_URL = "http://212.60.153.174:5000/";
    private static final long BACK_INTERVAL_MS = 2000;

    private WebView webView;
    private long lastBackAt = 0;

    @SuppressLint("SetJavaScriptEnabled")
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        webView = new WebView(this);
        setContentView(webView);

        WebSettings ws = webView.getSettings();
        ws.setJavaScriptEnabled(true);
        ws.setDomStorageEnabled(true);
        ws.setMediaPlaybackRequiresUserGesture(false);
        ws.setMixedContentMode(WebSettings.MIXED_CONTENT_ALWAYS_ALLOW);
        ws.setCacheMode(WebSettings.LOAD_DEFAULT);
        ws.setUserAgentString(ws.getUserAgentString() + " CameraRS-Android/3.1");

        webView.setWebViewClient(new WebViewClient() {
            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                // 站内链接留在 WebView，外部 http(s) 一律内嵌打开
                return false;
            }
        });
        webView.setWebChromeClient(new WebChromeClient());

        // 返回键：WebView 可后退则后退，否则双击退出
        getOnBackPressedDispatcher().addCallback(this, new OnBackPressedCallback(true) {
            @Override
            public void handleOnBackPressed() {
                if (webView.canGoBack()) {
                    webView.goBack();
                } else {
                    long now = System.currentTimeMillis();
                    if (now - lastBackAt < BACK_INTERVAL_MS) {
                        finish();
                    } else {
                        lastBackAt = now;
                        android.widget.Toast.makeText(MainActivity.this,
                                "再按一次退出", android.widget.Toast.LENGTH_SHORT).show();
                    }
                }
            }
        });

        if (savedInstanceState != null) {
            webView.restoreState(savedInstanceState);
        } else {
            webView.loadUrl(DEFAULT_URL);
        }
    }

    @Override
    protected void onSaveInstanceState(Bundle outState) {
        super.onSaveInstanceState(outState);
        webView.saveState(outState);
    }

    @Override
    protected void onPause() {
        webView.onPause();
        super.onPause();
    }

    @Override
    protected void onResume() {
        super.onResume();
        webView.onResume();
    }
}
