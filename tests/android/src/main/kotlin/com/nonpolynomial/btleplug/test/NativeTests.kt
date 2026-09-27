package com.nonpolynomial.btleplug.test

/// JNI bindings to the Rust integration test functions.
object NativeTests {
    external fun initBtleplug()

    // Adapter capabilities
    external fun testAdapterAddress()

    // Discovery
    external fun testDiscoverPeripheralByName()
    external fun testDiscoverServices()
    external fun testDiscoverCharacteristics()
    external fun testScanFilterByServiceUuid()
    external fun testAdvertisementManufacturerData()
    external fun testAdvertisementServices()
    external fun testAdvertisementServiceData128bit()

    // Retrieval
    external fun testRetrievePeripheralsNotSupported()

    // Connection
    external fun testConnectAndDisconnect()
    external fun testReconnectAfterDisconnect()
    external fun testPeripheralTriggeredDisconnect()
    external fun testReconnectAfterPeripheralTriggeredDisconnect()

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
    external fun testResubscribeDoesNotDuplicateNotifications()

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

    // Concurrency
    external fun testConcurrentConnectAndDiscover()
    external fun testConcurrentOperationsSameService()
    external fun testDiscoverServicesDuringRead()
    external fun testOperationsAcrossPeripheralTriggeredDisconnect()
    external fun testAddPeripheralByAddress()

    // GATT errors
    external fun testGattErrorStatusIsReported()
    external fun testRefusedSubscribeReturnsError()

    // GATT profile structure
    external fun testDiscoveryWithIncludedService()
}
