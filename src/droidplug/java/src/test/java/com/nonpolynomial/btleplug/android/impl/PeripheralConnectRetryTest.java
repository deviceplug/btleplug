package com.nonpolynomial.btleplug.android.impl;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

/**
 * Plain JVM unit tests for Peripheral's connect-retry decision. This exercises only the pure
 * {@link Peripheral#shouldRetryConnect(int, int, long)} helper, so it needs no Android framework
 * classes and runs under the regular {@code testDebugUnitTest} task.
 */
public class PeripheralConnectRetryTest {

    @Test
    public void retriesOnGattErrorWithinAttemptsAndTime() {
        assertTrue(Peripheral.shouldRetryConnect(Peripheral.GATT_ERROR, 0, 0));
        assertTrue(Peripheral.shouldRetryConnect(
                Peripheral.GATT_ERROR, Peripheral.MAX_CONNECT_RETRIES - 1, Peripheral.CONNECT_RETRY_MAX_ELAPSED_MS - 1));
    }

    @Test
    public void retriesOnHciConnectionFailedEstablishmentWithinAttemptsAndTime() {
        assertTrue(Peripheral.shouldRetryConnect(
                Peripheral.HCI_ERR_CONNECTION_FAILED_ESTABLISHMENT, 0, 0));
        assertTrue(Peripheral.shouldRetryConnect(
                Peripheral.HCI_ERR_CONNECTION_FAILED_ESTABLISHMENT,
                Peripheral.MAX_CONNECT_RETRIES - 1,
                Peripheral.CONNECT_RETRY_MAX_ELAPSED_MS - 1));
    }

    @Test
    public void doesNotRetryOnOtherStatuses() {
        assertFalse(Peripheral.shouldRetryConnect(8 /* GATT_CONN_TIMEOUT */, 0, 0));
        assertFalse(Peripheral.shouldRetryConnect(19 /* GATT_CONN_TERMINATE_PEER_USER */, 0, 0));
        assertFalse(Peripheral.shouldRetryConnect(0 /* GATT_SUCCESS, shouldn't reach here anyway */, 0, 0));
    }

    @Test
    public void doesNotRetryAtMaxConnectRetries() {
        assertFalse(Peripheral.shouldRetryConnect(Peripheral.GATT_ERROR, Peripheral.MAX_CONNECT_RETRIES, 0));
        assertFalse(Peripheral.shouldRetryConnect(
                Peripheral.HCI_ERR_CONNECTION_FAILED_ESTABLISHMENT, Peripheral.MAX_CONNECT_RETRIES, 0));
        assertFalse(Peripheral.shouldRetryConnect(Peripheral.GATT_ERROR, Peripheral.MAX_CONNECT_RETRIES + 5, 0));
    }

    @Test
    public void doesNotRetryWhenElapsedAtOrAboveMax() {
        assertFalse(Peripheral.shouldRetryConnect(
                Peripheral.GATT_ERROR, 0, Peripheral.CONNECT_RETRY_MAX_ELAPSED_MS));
        assertFalse(Peripheral.shouldRetryConnect(
                Peripheral.GATT_ERROR, 0, Peripheral.CONNECT_RETRY_MAX_ELAPSED_MS + 1000));
        assertFalse(Peripheral.shouldRetryConnect(
                Peripheral.HCI_ERR_CONNECTION_FAILED_ESTABLISHMENT, 0, Peripheral.CONNECT_RETRY_MAX_ELAPSED_MS));
    }
}
