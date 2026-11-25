package com.pream.portalkeeper;

import android.Manifest;
import android.app.Activity;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.PowerManager;
import android.provider.Settings;
import android.text.InputType;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

public class MainActivity extends Activity {
    private final EditText[] ssid = new EditText[Config.SLOTS];
    private final EditText[] user = new EditText[Config.SLOTS];
    private final EditText[] pass = new EditText[Config.SLOTS];
    private TextView status;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final Runnable refresh = new Runnable() {
        @Override
        public void run() {
            if (status != null) status.setText(KeeperService.RUNNING
                    ? KeeperService.LAST
                    : "Stopped — press Start");
            handler.postDelayed(this, 1000);
        }
    };

    private int dp(int v) {
        return Math.round(v * getResources().getDisplayMetrics().density);
    }

    @Override
    protected void onCreate(Bundle b) {
        super.onCreate(b);
        Config.ensureDefaults(this);

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        int p = dp(18);
        root.setPadding(p, p, p, p);

        TextView title = new TextView(this);
        title.setText("Portal Keeper");
        title.setTextSize(24);
        root.addView(title);

        TextView hint = new TextView(this);
        hint.setText("One account per Wi-Fi. Sign-in follows whichever network you are on.");
        hint.setTextSize(13);
        hint.setPadding(0, dp(4), 0, dp(12));
        root.addView(hint);

        status = new TextView(this);
        status.setTextSize(15);
        status.setPadding(0, dp(8), 0, dp(14));
        root.addView(status);

        for (int i = 0; i < Config.SLOTS; i++) {
            root.addView(label("Wi-Fi " + (i + 1) + " name"));
            ssid[i] = field(Config.ssid(this, i), InputType.TYPE_CLASS_TEXT);
            root.addView(ssid[i]);
            root.addView(label("Username"));
            user[i] = field(Config.user(this, i), InputType.TYPE_CLASS_TEXT);
            root.addView(user[i]);
            root.addView(label("Password"));
            pass[i] = field(Config.pass(this, i),
                    InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);
            root.addView(pass[i]);
        }

        Button save = new Button(this);
        save.setText("Save accounts");
        save.setOnClickListener(v -> {
            for (int i = 0; i < Config.SLOTS; i++) {
                Config.save(this, i, ssid[i].getText().toString(),
                        user[i].getText().toString(), pass[i].getText().toString());
            }
            toast("Saved");
        });
        root.addView(save);

        Button toggle = new Button(this);
        toggle.setText("Start / stop keeper");
        toggle.setOnClickListener(v -> {
            if (KeeperService.RUNNING) {
                stopService(new Intent(this, KeeperService.class));
                toast("Stopped");
            } else {
                startSvc();
                toast("Started");
            }
        });
        root.addView(toggle);

        Button battery = new Button(this);
        battery.setText("Allow background (battery)");
        battery.setOnClickListener(v -> {
            try {
                PowerManager pm = (PowerManager) getSystemService(POWER_SERVICE);
                if (pm != null && !pm.isIgnoringBatteryOptimizations(getPackageName())) {
                    startActivity(new Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS,
                            Uri.parse("package:" + getPackageName())));
                } else {
                    toast("Already allowed");
                }
            } catch (Exception e) {
                toast("Open Settings > Apps > Portal Keeper > Battery");
            }
        });
        root.addView(battery);

        ScrollView scroll = new ScrollView(this);
        scroll.addView(root, new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        setContentView(scroll);

        askPermissions();
        handler.post(refresh);
    }

    @Override
    protected void onDestroy() {
        handler.removeCallbacks(refresh);
        super.onDestroy();
    }

    private TextView label(String text) {
        TextView t = new TextView(this);
        t.setText(text);
        t.setTextSize(12);
        t.setPadding(0, dp(10), 0, dp(2));
        return t;
    }

    private EditText field(String value, int type) {
        EditText e = new EditText(this);
        e.setText(value);
        e.setInputType(type);
        e.setSingleLine(true);
        return e;
    }

    private void toast(String text) {
        Toast.makeText(this, text, Toast.LENGTH_SHORT).show();
    }

    private void startSvc() {
        Intent svc = new Intent(this, KeeperService.class);
        if (Build.VERSION.SDK_INT >= 26) startForegroundService(svc);
        else startService(svc);
    }

    private void askPermissions() {
        java.util.List<String> want = new java.util.ArrayList<>();
        if (Build.VERSION.SDK_INT >= 23
                && checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) != PackageManager.PERMISSION_GRANTED) {
            want.add(Manifest.permission.ACCESS_FINE_LOCATION);
        }
        if (Build.VERSION.SDK_INT >= 33
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            want.add(Manifest.permission.POST_NOTIFICATIONS);
        }
        if (!want.isEmpty()) {
            requestPermissions(want.toArray(new String[0]), 1);
        }
        if (Config.enabled(this) && !KeeperService.RUNNING) startSvc();
    }
}
