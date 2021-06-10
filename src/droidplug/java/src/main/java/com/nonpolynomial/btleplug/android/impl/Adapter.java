package com.nonpolynomial.btleplug.android.impl;

import android.bluetooth.BluetoothAdapter;
import android.bluetooth.le.ScanCallback;
import android.bluetooth.le.ScanResult;
import android.bluetooth.le.ScanSettings;

@SuppressWarnings("unused") // Native code uses this class.
class Adapter {
    private long handle;
    private final Callback callback = new Callback();

    public Adapter() {}

    public void startScan() {
        ScanSettings settings = new ScanSettings.Builder()
                .setCallbackType(ScanSettings.CALLBACK_TYPE_ALL_MATCHES)
                .build();
        BluetoothAdapter.getDefaultAdapter().getBluetoothLeScanner().startScan(null, settings, this.callback);
    }

    public void stopScan() {
        BluetoothAdapter.getDefaultAdapter().getBluetoothLeScanner().stopScan(this.callback);
    }

    private native void reportScanResult(ScanResult result);

    public native void onConnectionStateChanged(String address, boolean connected);

    private class Callback extends ScanCallback {
        @Override
        public void onScanResult(int callbackType, ScanResult result) {
            Adapter.this.reportScanResult(result);
        }
    }
}
