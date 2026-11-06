package @PKG@;

import android.app.NotificationManager;
import android.content.Intent;
import android.os.Build;
import android.os.IBinder;
import android.os.PowerManager;

// The foreground service behind `j3.android.service.keepAlive`: it keeps the process (and its
// sockets) alive in the background, with a partial wake lock so the CPU runs with the screen off.
public class J3Service extends android.app.Service {
    static final int ID = 0x4a330001;
    private PowerManager.WakeLock lock;

    @Override
    public IBinder onBind(Intent i) {
        return null;
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        try {
            J3Activity.channel(this, "j3.keepalive", "Running in the background", NotificationManager.IMPORTANCE_LOW);
            CharSequence label = getApplicationInfo().loadLabel(getPackageManager());
            android.app.Notification n = J3Activity.builder(this, "j3.keepalive", ID, label.toString(), "Running in the background").setOngoing(true).build();
            if (Build.VERSION.SDK_INT >= 29) {
                startForeground(ID, n, @FGS_TYPE@);
            } else {
                startForeground(ID, n);
            }
            if (lock == null) {
                PowerManager pm = (PowerManager) getSystemService(POWER_SERVICE);
                lock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, getPackageName() + ":keepalive");
                lock.acquire();
            }
        } catch (Exception e) {
            stopSelf();
        }
        return START_NOT_STICKY;
    }

    @Override
    public void onDestroy() {
        if (lock != null && lock.isHeld()) {
            lock.release();
        }
        lock = null;
        super.onDestroy();
    }
}
