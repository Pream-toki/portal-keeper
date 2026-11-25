package com.pream.portalkeeper;

import android.content.Context;
import android.content.SharedPreferences;

final class Config {
    static final int SLOTS = 2;

    private static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences("pk", Context.MODE_PRIVATE);
    }

    static void ensureDefaults(Context c) {
        SharedPreferences s = prefs(c);
        if (s.getBoolean("seeded", false)) return;
        String[] v = {"KBU.WiFi", "", "", "KBU.FL7", "", ""};
        try (java.io.InputStream in = c.getAssets().open("seed.properties")) {
            java.util.Properties p = new java.util.Properties();
            p.load(in);
            for (int i = 0; i < SLOTS; i++) {
                v[i * 3] = p.getProperty("ssid" + (i + 1), v[i * 3]);
                v[i * 3 + 1] = p.getProperty("user" + (i + 1), "");
                v[i * 3 + 2] = p.getProperty("pass" + (i + 1), "");
            }
        } catch (Exception ignored) {
        }
        SharedPreferences.Editor e = s.edit().putBoolean("seeded", true);
        for (int i = 0; i < SLOTS; i++) {
            e.putString("ssid" + (i + 1), v[i * 3]);
            e.putString("user" + (i + 1), v[i * 3 + 1]);
            e.putString("pass" + (i + 1), v[i * 3 + 2]);
        }
        e.apply();
    }

    static String ssid(Context c, int i) {
        return prefs(c).getString("ssid" + (i + 1), "");
    }

    static String user(Context c, int i) {
        return prefs(c).getString("user" + (i + 1), "");
    }

    static String pass(Context c, int i) {
        return prefs(c).getString("pass" + (i + 1), "");
    }

    static void save(Context c, int i, String ssid, String user, String pass) {
        prefs(c).edit()
                .putString("ssid" + (i + 1), ssid.trim())
                .putString("user" + (i + 1), user.trim())
                .putString("pass" + (i + 1), pass)
                .apply();
    }

    static boolean enabled(Context c) {
        return prefs(c).getBoolean("enabled", true);
    }

    static void setEnabled(Context c, boolean on) {
        prefs(c).edit().putBoolean("enabled", on).apply();
    }

    static int indexOf(Context c, String ssid) {
        String n = norm(ssid);
        if (n.isEmpty()) return -1;
        for (int i = 0; i < SLOTS; i++) {
            if (norm(ssid(c, i)).equals(n)) return i;
        }
        return -1;
    }

    static String norm(String s) {
        return s == null ? "" : s.replaceAll("[^A-Za-z0-9]", "").toLowerCase();
    }
}
