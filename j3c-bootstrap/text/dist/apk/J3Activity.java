package @PKG@;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.VibrationEffect;
import android.os.Vibrator;

// SDL's activity, with our library list, `J3_NATIVE_LIB_DIR` set before SDL starts `main` (where
// `[android] native_libs` land), and the methods `j3.android` calls through JNI (any thread).
public class J3Activity extends org.libsdl.app.SDLActivity {
    static final String TAP = "j3.notify.tap";
    static final boolean HAS_SERVICE = @HAS_SERVICE@;
    private volatile boolean tapped;
    private boolean asked;

    @Override
    protected String[] getLibraries() {
        return new String[] { @LIBS@ };
    }

    @Override
    protected void onCreate(android.os.Bundle saved) {
        try {
            android.system.Os.setenv("J3_NATIVE_LIB_DIR", getApplicationInfo().nativeLibraryDir, true);
        } catch (Exception e) {
        }
        noteTap(getIntent());
        super.onCreate(saved);
    }

    @Override
    protected void onNewIntent(Intent i) {
        super.onNewIntent(i);
        noteTap(i);
    }

    private void noteTap(Intent i) {
        if (i != null && TAP.equals(i.getAction())) {
            tapped = true;
        }
    }

    @Override
    protected void onDestroy() {
        j3KeepAlive(false);
        super.onDestroy();
    }

    public boolean j3Vibrate(int ms) {
        try {
            Vibrator v = (Vibrator) getSystemService(Context.VIBRATOR_SERVICE);
            if (v == null || !v.hasVibrator() || ms <= 0) {
                return false;
            }
            v.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE));
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    /// The channel `id` (created once), or null when notifications are off or not allowed yet.
    static NotificationManager channel(Context cx, String id, String name, int importance) {
        NotificationManager nm = (NotificationManager) cx.getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm == null) {
            return null;
        }
        nm.createNotificationChannel(new NotificationChannel(id, name, importance));
        return nm;
    }

    /// A notification that brings this activity to the front when tapped.
    static Notification.Builder builder(Context cx, String channel, int id, String title, String text) {
        Intent i = new Intent(cx, J3Activity.class).setAction(TAP).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent pi = PendingIntent.getActivity(cx, id, i, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        int icon = cx.getApplicationInfo().icon;
        return new Notification.Builder(cx, channel)
            .setSmallIcon(icon != 0 ? icon : android.R.drawable.ic_dialog_info)
            .setContentTitle(title)
            .setContentText(text)
            .setContentIntent(pi);
    }

    private boolean allowed() {
        if (Build.VERSION.SDK_INT < 33 || checkSelfPermission("android.permission.POST_NOTIFICATIONS") == PackageManager.PERMISSION_GRANTED) {
            return true;
        }
        if (!asked) {
            asked = true;
            runOnUiThread(new Runnable() {
                public void run() {
                    requestPermissions(new String[] { "android.permission.POST_NOTIFICATIONS" }, 0x4a33);
                }
            });
        }
        return false;
    }

    public boolean j3Notify(int id, String title, String text) {
        try {
            if (!allowed()) {
                return false;
            }
            NotificationManager nm = channel(this, "j3", "Alerts", NotificationManager.IMPORTANCE_HIGH);
            if (nm == null || !nm.areNotificationsEnabled()) {
                return false;
            }
            nm.notify(id, builder(this, "j3", id, title, text).setAutoCancel(true).build());
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    public boolean j3NotifyCancel(int id) {
        NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm == null) {
            return false;
        }
        nm.cancel(id);
        return true;
    }

    public boolean j3NotifyTapped() {
        boolean t = tapped;
        tapped = false;
        return t;
    }

    public boolean j3KeepAlive(boolean on) {
        if (!HAS_SERVICE) {
            return false;
        }
        try {
            Intent i = new Intent(this, J3Service.class);
            if (on) {
                allowed();
                startForegroundService(i);
            } else {
                stopService(i);
            }
            return true;
        } catch (Exception e) {
            return false;
        }
    }
}
