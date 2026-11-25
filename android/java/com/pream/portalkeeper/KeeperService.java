package com.pream.portalkeeper;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.wifi.WifiInfo;
import android.net.wifi.WifiManager;
import android.os.Build;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.IBinder;

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

    private HandlerThread thread;
    private Handler handler;
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
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(CHANNEL_ID, note("Watching Wi-Fi"),
                    android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION);
        } else {
            startForeground(CHANNEL_ID, note("Watching Wi-Fi"));
        }
        thread = new HandlerThread("keeper");
        thread.start();
        handler = new Handler(thread.getLooper());
        handler.post(tick);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        return START_STICKY;
    }

    @Override
    public void onDestroy() {
        RUNNING = false;
        if (handler != null) handler.removeCallbacks(tick);
        if (thread != null) thread.quitSafely();
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
        android.util.Log.i("PK", text);
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

    private static String clean(String s) {
        if (s == null) return "";
        s = s.trim();
        if (s.startsWith("\"") && s.endsWith("\"") && s.length() > 1) {
            s = s.substring(1, s.length() - 1);
        }
        return s.equals("<unknown ssid>") ? "" : s;
    }

    /** Two sources, because OEM builds redact one or the other. */
    private String currentSsid(Network n) {
        WifiManager wm = (WifiManager) getApplicationContext().getSystemService(WIFI_SERVICE);
        WifiInfo ci = wm.getConnectionInfo();
        String fromManager = ci == null ? "" : clean(ci.getSSID());
        String fromCaps = "";
        ConnectivityManager cm = (ConnectivityManager) getSystemService(CONNECTIVITY_SERVICE);
        NetworkCapabilities c = cm.getNetworkCapabilities(n);
        if (c != null) {
            android.net.TransportInfo ti = c.getTransportInfo();
            if (ti instanceof WifiInfo) fromCaps = clean(((WifiInfo) ti).getSSID());
        }
        if (fromManager.isEmpty() && fromCaps.isEmpty()) android.util.Log.w("PK", "ssid empty from both sources");
        return fromManager.isEmpty() ? fromCaps : fromManager;
    }

    private String gateway(Network n) {
        ConnectivityManager cm = (ConnectivityManager) getSystemService(CONNECTIVITY_SERVICE);
        android.net.LinkProperties lp = cm.getLinkProperties(n);
        if (lp == null) return "";
        for (android.net.RouteInfo r : lp.getRoutes()) {
            if (r.isDefaultRoute() && r.getGateway() != null) {
                String g = r.getGateway().getHostAddress();
                if (g != null && g.contains(".")) return g;
            }
        }
        return "";
    }

    private void check() {
        if (!Config.enabled(this)) {
            status("Paused");
            return;
        }
        Network wifi = bindWifi();
        if (wifi == null) {
            status("No Wi-Fi");
            return;
        }
        String ssid = currentSsid(wifi);
        if (online()) {
            status(ssid.isEmpty() ? "Signed in" : "Signed in · " + ssid);
            return;
        }
        if (ssid.isEmpty()) {
            status("Offline · Wi-Fi name unavailable");
            return;
        }
        int slot = Config.indexOf(this, ssid);
        if (slot < 0) {
            status("Not a saved network · " + ssid);
            return;
        }
        status("Signing in · " + ssid);
        boolean ok = login(slot, wifi);
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
            android.util.Log.w("PK", "probe: " + e);
            return false;
        } finally {
            if (c != null) c.disconnect();
        }
    }

    private boolean login(int slot, Network wifi) {
        String gw = gateway(wifi);
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
            android.util.Log.w("PK", "login gw=" + gw + ": " + e);
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
