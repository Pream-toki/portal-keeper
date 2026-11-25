package com.pream.portalkeeper;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.net.ConnectivityManager;
import android.net.DhcpInfo;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.wifi.WifiInfo;
import android.net.wifi.WifiManager;
import android.os.Build;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;

import java.io.BufferedReader;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.net.URLEncoder;

public class KeeperService extends Service {
    static volatile boolean RUNNING = false;
    static volatile String LAST = "starting";

    private static final String PROBE = "http://connectivitycheck.gstatic.com/generate_204";
    private static final int CHANNEL_ID = 1;

    private final Handler handler = new Handler(Looper.getMainLooper());
    private final Runnable tick = new Runnable() {
        @Override
        public void run() {
            try {
                check();
            } catch (Throwable ignored) {
            }
            handler.postDelayed(this, 10000);
        }
    };

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    @Override
    public void onCreate() {
        super.onCreate();
        RUNNING = true;
        NotificationChannel ch = new NotificationChannel(
                "keeper", "Portal Keeper", NotificationManager.IMPORTANCE_LOW);
        ch.setShowBadge(false);
        ((NotificationManager) getSystemService(NOTIFICATION_SERVICE)).createNotificationChannel(ch);
        startForeground(CHANNEL_ID, note("Watching Wi-Fi"));
        handler.post(tick);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        return START_STICKY;
    }

    @Override
    public void onDestroy() {
        RUNNING = false;
        handler.removeCallbacks(tick);
        super.onDestroy();
    }

    private Notification note(String text) {
        Notification.Builder b = Build.VERSION.SDK_INT >= 26
                ? new Notification.Builder(this, "keeper")
                : new Notification.Builder(this);
        b.setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle("Portal Keeper")
                .setContentText(text)
                .setOngoing(true);
        return b.build();
    }

    private void status(String text) {
        LAST = text;
        try {
            ((NotificationManager) getSystemService(NOTIFICATION_SERVICE))
                    .notify(CHANNEL_ID, note(text));
        } catch (Throwable ignored) {
        }
    }

    /** Bind all our HTTP to the Wi-Fi network so mobile data can't hide a signed-out portal. */
    private Network bindWifi() {
        ConnectivityManager cm = (ConnectivityManager) getSystemService(CONNECTIVITY_SERVICE);
        Network wifi = null;
        for (Network n : cm.getAllNetworks()) {
            NetworkCapabilities c = cm.getNetworkCapabilities(n);
            if (c != null && c.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) {
                wifi = n;
                break;
            }
        }
        cm.bindProcessToNetwork(wifi);
        return wifi;
    }

    private String currentSsid() {
        WifiManager wm = (WifiManager) getApplicationContext().getSystemService(WIFI_SERVICE);
        WifiInfo info = wm.getConnectionInfo();
        if (info == null) return "";
        String s = info.getSSID();
        if (s == null) return "";
        s = s.trim();
        if (s.startsWith("\"") && s.endsWith("\"") && s.length() > 1) {
            s = s.substring(1, s.length() - 1);
        }
        return s.equals("<unknown ssid>") ? "" : s;
    }

    private String gateway() {
        WifiManager wm = (WifiManager) getApplicationContext().getSystemService(WIFI_SERVICE);
        DhcpInfo d = wm.getDhcpInfo();
        if (d == null || d.gateway == 0) return "";
        int g = d.gateway;
        return (g & 0xff) + "." + ((g >> 8) & 0xff) + "." + ((g >> 16) & 0xff) + "." + ((g >> 24) & 0xff);
    }

    private void check() {
        if (!Config.enabled(this)) {
            status("Paused");
            return;
        }
        if (bindWifi() == null) {
            status("No Wi-Fi");
            return;
        }
        String ssid = currentSsid();
        if (ssid.isEmpty()) {
            status("Wi-Fi name hidden — grant Location permission");
            return;
        }
        int slot = Config.indexOf(this, ssid);
        if (slot < 0) {
            status("Not a saved network · " + ssid);
            return;
        }
        if (online()) {
            status("Signed in · " + ssid);
            return;
        }
        status("Signing in · " + ssid);
        boolean ok = login(slot);
        status((ok ? "Signed in · " : "Retry · ") + ssid);
    }

    private boolean online() {
        HttpURLConnection c = null;
        try {
            c = (HttpURLConnection) new URL(PROBE).openConnection();
            c.setConnectTimeout(4000);
            c.setReadTimeout(4000);
            c.setInstanceFollowRedirects(false);
            int code = c.getResponseCode();
            return code == 204 || code == 200;
        } catch (Exception e) {
            return false;
        } finally {
            if (c != null) c.disconnect();
        }
    }

    private boolean login(int slot) {
        String gw = gateway();
        if (gw.isEmpty()) return false;
        HttpURLConnection c = null;
        try {
            String body = "username=" + URLEncoder.encode(Config.user(this, slot), "UTF-8")
                    + "&password=" + URLEncoder.encode(Config.pass(this, slot), "UTF-8")
                    + "&dst=" + URLEncoder.encode(PROBE, "UTF-8")
                    + "&popup=true";
            c = (HttpURLConnection) new URL("http://" + gw + "/login").openConnection();
            c.setRequestMethod("POST");
            c.setDoOutput(true);
            c.setConnectTimeout(6000);
            c.setReadTimeout(6000);
            c.setInstanceFollowRedirects(false);
            c.setRequestProperty("Content-Type", "application/x-www-form-urlencoded");
            c.setFixedLengthStreamingMode(body.getBytes("UTF-8").length);
            OutputStream out = c.getOutputStream();
            out.write(body.getBytes("UTF-8"));
            out.close();
            int code = c.getResponseCode();
            drain(code >= 400 ? c.getErrorStream() : c.getInputStream());
            Thread.sleep(400);
            return online();
        } catch (Exception e) {
            return false;
        } finally {
            if (c != null) c.disconnect();
        }
    }

    private void drain(InputStream in) {
        try (BufferedReader r = new BufferedReader(new InputStreamReader(in))) {
            while (r.readLine() != null) {
            }
        } catch (Exception ignored) {
        }
    }
}
