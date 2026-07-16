package com.nonpolynomial.btleplug.test

/// JNI bindings to the Rust integration test functions.
object NativeTests {
    external fun initBtleplug()

    // Discovery
    external fun testDiscoverPeripheralByName()
    external fun testDiscoverServices()
    external fun testDiscoverCharacteristics()
    external fun testScanFilterByServiceUuid()
    external fun testAdvertisementManufacturerData()
    external fun testAdvertisementServices()

    // Connection
    external fun testConnectAndDisconnect()
    external fun testReconnectAfterDisconnect()
    external fun testPeripheralTriggeredDisconnect()

    // Read/Write
    external fun testReadStaticValue()
    external fun testReadCounterIncrements()
    external fun testWriteWithResponse()
    external fun testWriteWithoutResponse()
    external fun testWriteWithoutResponseBurst()
    external fun testReadWriteRoundtrip()
    external fun testLongValueReadWrite()
    external fun testCharacteristicProperties()

    // Notifications
    external fun testSubscribeAndReceiveNotifications()
    external fun testSubscribeAndReceiveIndications()
    external fun testUnsubscribeStopsNotifications()
    external fun testConfigurableNotificationPayload()

    // Descriptors
    external fun testReadOnlyDescriptor()
    external fun testReadWriteDescriptorRoundtrip()
    external fun testDescriptorDiscovery()

    // Device Info
    external fun testMtuAfterServiceDiscovery()
    external fun testReadRssi()
    external fun testPropertiesContainPeripheralInfo()
    external fun testConnectionParameters()
    external fun testRequestConnectionParameters()
}
