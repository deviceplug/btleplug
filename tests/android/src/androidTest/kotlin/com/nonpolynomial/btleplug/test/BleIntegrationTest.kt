package com.nonpolynomial.btleplug.test

import android.content.Intent
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import org.junit.Before
import org.junit.BeforeClass
import org.junit.ClassRule
import org.junit.Rule
import org.junit.Test
import org.junit.rules.Timeout
import org.junit.runner.RunWith

/// Instrumentation tests that call through JNI to the shared Rust test logic.
///
/// Each @Test method calls a single native function which runs the corresponding
/// async test on a process-global tokio runtime. Tests run sequentially by default
/// (JUnit4 single-threaded runner).
@RunWith(AndroidJUnit4::class)
class BleIntegrationTest {

    @get:Rule
    val timeout: Timeout = Timeout.seconds(60)

    @Before
    fun cooldown() {
        Thread.sleep(1000)
    }

    companion object {
        init {
            System.loadLibrary("btleplug_android_tests")
        }

        @ClassRule
        @JvmField
        val permissionRule: GrantPermissionRule = GrantPermissionRule.grant(
            android.Manifest.permission.BLUETOOTH_SCAN,
            android.Manifest.permission.BLUETOOTH_CONNECT,
            android.Manifest.permission.ACCESS_FINE_LOCATION,
        )

        @BeforeClass
        @JvmStatic
        fun setup() {
            // Start a foreground service to keep the app in the foreground.
            // On Android 12+, BLE scan results require ACCESS_FINE_LOCATION which
            // is only granted to foreground apps. The instrumentation runner kills
            // activities, so a foreground service is the reliable approach.
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val serviceIntent = Intent(context, BleTestService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(serviceIntent)
            } else {
                context.startService(serviceIntent)
            }
            Thread.sleep(1000) // wait for service to start and foreground status to take effect

            NativeTests.initBtleplug()
        }
    }

    // ── Adapter capabilities ────────────────────────────────────────
    @Test fun testAdapterAddress() = NativeTests.testAdapterAddress()

    // ── Discovery ───────────────────────────────────────────────────
    @Test fun testDiscoverPeripheralByName() = NativeTests.testDiscoverPeripheralByName()
    @Test fun testDiscoverServices() = NativeTests.testDiscoverServices()
    @Test fun testDiscoverCharacteristics() = NativeTests.testDiscoverCharacteristics()
    @Test fun testScanFilterByServiceUuid() = NativeTests.testScanFilterByServiceUuid()
    @Test fun testAdvertisementManufacturerData() = NativeTests.testAdvertisementManufacturerData()
    @Test fun testAdvertisementServices() = NativeTests.testAdvertisementServices()

    // ── Retrieval ────────────────────────────────────────────────────
    @Test fun testRetrievePeripheralsNotSupported() = NativeTests.testRetrievePeripheralsNotSupported()

    // ── Connection ──────────────────────────────────────────────────
    @Test fun testConnectAndDisconnect() = NativeTests.testConnectAndDisconnect()
    @Test fun testReconnectAfterDisconnect() = NativeTests.testReconnectAfterDisconnect()
    @Test fun testPeripheralTriggeredDisconnect() = NativeTests.testPeripheralTriggeredDisconnect()

    // ── Read/Write ──────────────────────────────────────────────────
    @Test fun testReadStaticValue() = NativeTests.testReadStaticValue()
    @Test fun testReadCounterIncrements() = NativeTests.testReadCounterIncrements()
    @Test fun testWriteWithResponse() = NativeTests.testWriteWithResponse()
    @Test fun testWriteWithoutResponse() = NativeTests.testWriteWithoutResponse()
    @Test fun testWriteWithoutResponseBurst() = NativeTests.testWriteWithoutResponseBurst()
    @Test fun testReadWriteRoundtrip() = NativeTests.testReadWriteRoundtrip()
    @Test fun testLongValueReadWrite() = NativeTests.testLongValueReadWrite()
    @Test fun testCharacteristicProperties() = NativeTests.testCharacteristicProperties()

    // ── Notifications ───────────────────────────────────────────────
    @Test fun testSubscribeAndReceiveNotifications() = NativeTests.testSubscribeAndReceiveNotifications()
    @Test fun testSubscribeAndReceiveIndications() = NativeTests.testSubscribeAndReceiveIndications()
    @Test fun testUnsubscribeStopsNotifications() = NativeTests.testUnsubscribeStopsNotifications()
    @Test fun testConfigurableNotificationPayload() = NativeTests.testConfigurableNotificationPayload()

    // ── Descriptors ─────────────────────────────────────────────────
    @Test fun testReadOnlyDescriptor() = NativeTests.testReadOnlyDescriptor()
    @Test fun testReadWriteDescriptorRoundtrip() = NativeTests.testReadWriteDescriptorRoundtrip()
    @Test fun testDescriptorDiscovery() = NativeTests.testDescriptorDiscovery()

    // ── Device Info ─────────────────────────────────────────────────
    @Test fun testMtuAfterConnection() = NativeTests.testMtuAfterConnection()
    @Test fun testReadRssi() = NativeTests.testReadRssi()
    @Test fun testPropertiesContainPeripheralInfo() = NativeTests.testPropertiesContainPeripheralInfo()
    @Test fun testConnectionParameters() = NativeTests.testConnectionParameters()
    @Test fun testRequestConnectionParameters() = NativeTests.testRequestConnectionParameters()
}
