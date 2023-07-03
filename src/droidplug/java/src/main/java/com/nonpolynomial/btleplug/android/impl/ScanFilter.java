package com.nonpolynomial.btleplug.android.impl;

import java.util.Arrays;

public class ScanFilter {
    private final String[] uuids;

    public ScanFilter(String uuids[]) {
        if (uuids == null) {
            this.uuids = new String[0];
        } else {
            int len = uuids.length;
            this.uuids = Arrays.copyOf(uuids, len);
        }
    }

    public String[] getUuids() {
        int len = uuids.length;
        return Arrays.copyOf(uuids, len);
    }
}
