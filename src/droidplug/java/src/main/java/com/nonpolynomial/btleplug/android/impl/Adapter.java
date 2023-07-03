package com.nonpolynomial.btleplug.android.impl;

import android.bluetooth.BluetoothAdapter;
import android.bluetooth.le.ScanCallback;
import android.bluetooth.le.ScanFilter.Builder;
import android.bluetooth.le.ScanResult;
import android.bluetooth.le.ScanSettings;
import android.os.ParcelUuid;

import java.util.ArrayList;
import java.util.List;

@SuppressWarnings("unused") // Native code uses this class.
class Adapter {
    private long handle;
    private final Callback callback = new Callback();

    public Adapter() {}

    public void startScan(ScanFilter filter) {
        ArrayList<android.bluetooth.le.ScanFilter> filters = null;
        String[] uuids = filter.getUuids();
        if (uuids.length > 0) {
            filters = new ArrayList<>();
            for (String uuid : uuids) {
                filters.add(new Builder().setServiceUuid(ParcelUuid.fromString(uuid)).build());
            }
        }
        ScanSettings settings = new ScanSettings.Builder()
                .setCallbackType(ScanSettings.CALLBACK_TYPE_ALL_MATCHES)
                .build();
        BluetoothAdapter.getDefaultAdapter().getBluetoothLeScanner().startScan(filters, settings, this.callback);
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
