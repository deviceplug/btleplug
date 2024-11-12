package com.nonpolynomial.btleplug.android.impl;

import android.annotation.SuppressLint;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothDevice;
import android.bluetooth.BluetoothGatt;
import android.bluetooth.BluetoothGattCallback;
import android.bluetooth.BluetoothGattCharacteristic;
import android.bluetooth.BluetoothGattDescriptor;
import android.bluetooth.BluetoothGattService;

import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.LinkedList;
import java.util.List;
import java.util.Queue;
import java.util.UUID;

import io.github.gedgygedgy.rust.future.Future;
import io.github.gedgygedgy.rust.stream.QueueStream;
import io.github.gedgygedgy.rust.future.SimpleFuture;
import io.github.gedgygedgy.rust.stream.Stream;

@SuppressWarnings("unused") // Native code uses this class.
class Peripheral {
    private static final UUID CLIENT_CHARACTERISTIC_CONFIGURATION_DESCRIPTOR = new UUID(0x00002902_0000_1000L, 0x8000_00805f9b34fbL);

    private final BluetoothDevice device;
    private final Adapter adapter;
    private BluetoothGatt gatt;
    private final Callback callback;
    private boolean connected = false;

    private final Queue<Runnable> commandQueue = new LinkedList<>();
    private final LinkedList<WeakReference<QueueStream<BluetoothGattCharacteristic>>> notificationStreams = new LinkedList<>();
    private boolean executingCommand = false;
    private CommandCallback commandCallback;

    public Peripheral(Adapter adapter, String address) {
        this.device = BluetoothAdapter.getDefaultAdapter().getRemoteDevice(address);
        this.adapter = adapter;
        this.callback = new Callback();
    }

    @SuppressLint("MissingPermission")
    public Future<Void> connect() {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> {
                this.asyncWithFuture(future, () -> {
                    CommandCallback callback = new CommandCallback() {
                        @Override
                        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (status != BluetoothGatt.GATT_SUCCESS) {
                                    throw new NotConnectedException();
                                }

                                if (newState == BluetoothGatt.STATE_CONNECTED) {
                                    Peripheral.this.wakeCommand(future, null);
                                }
                            });
                        }
                    };

                    if (this.connected) {
                        Peripheral.this.wakeCommand(future, null);
                    } else if (this.gatt == null) {
                        try {
                            this.setCommandCallback(callback);
                            this.gatt = this.device.connectGatt(null, false, this.callback);
                        } catch (SecurityException ex) {
                            throw new PermissionDeniedException(ex);
                        }
                    } else {
                        this.setCommandCallback(callback);
                        if (!this.gatt.connect()) {
                            throw new RuntimeException("Unable to reconnect to device");
                        }
                    }
                });
            });
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<Void> disconnect() {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> {
                this.asyncWithFuture(future, () -> {
                    if (!this.connected) {
                        Peripheral.this.wakeCommand(future, null);
                    } else {
                        this.setCommandCallback(new CommandCallback() {
                            @Override
                            public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
                                Peripheral.this.asyncWithFuture(future, () -> {
                                    if (status != BluetoothGatt.GATT_SUCCESS) {
                                        throw new RuntimeException("Unable to disconnect");
                                    }

                                    if (newState == BluetoothGatt.STATE_DISCONNECTED) {
                                        Peripheral.this.gatt.close();
                                        Peripheral.this.gatt = null;
                                        Peripheral.this.wakeCommand(future, null);
                                    }
                                });
                            }
                        });
                        this.gatt.disconnect();
                    }
                });
            });
        }
        return future;
    }

    public boolean isConnected() {
        return this.connected;
    }

    @SuppressLint("MissingPermission")
    public Future<byte[]> read(UUID uuid) {
        SimpleFuture<byte[]> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> {
                this.asyncWithFuture(future, () -> {
                    if (!this.connected) {
                        throw new NotConnectedException();
                    }

                    BluetoothGattCharacteristic characteristic = this.getCharacteristicByUuid(uuid);
                    this.setCommandCallback(new CommandCallback() {
                        @Override
                        public void onCharacteristicRead(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (!characteristic.getUuid().equals(uuid)) {
                                    throw new UnexpectedCharacteristicException();
                                }

                                Peripheral.this.wakeCommand(future, characteristic.getValue());
                            });
                        }
                        @Override
                        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (status != BluetoothGatt.GATT_SUCCESS) {
                                    throw new RuntimeException("Disconnected while in read operation");
                                }

                                if (newState == BluetoothGatt.STATE_DISCONNECTED) {
                                    Peripheral.this.gatt.close();
                                    Peripheral.this.gatt = null;
                                    Peripheral.this.wakeCommand(future, null);
                                }
                            });
                        }
                    });
                    if (!this.gatt.readCharacteristic(characteristic)) {
                        throw new RuntimeException("Unable to read characteristic");
                    }
                });
            });
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<Void> write(UUID uuid, byte[] data, int writeType) {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> {
                this.asyncWithFuture(future, () -> {
                    if (!this.connected) {
                        throw new NotConnectedException();
                    }

                    BluetoothGattCharacteristic characteristic = this.getCharacteristicByUuid(uuid);
                    characteristic.setValue(data);
                    characteristic.setWriteType(writeType);
                    this.setCommandCallback(new CommandCallback() {
                        @Override
                        public void onCharacteristicWrite(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (!characteristic.getUuid().equals(uuid)) {
                                    throw new UnexpectedCharacteristicException();
                                }

                                Peripheral.this.wakeCommand(future, null);
                            });
                        }
                        @Override
                        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (status != BluetoothGatt.GATT_SUCCESS) {
                                    throw new RuntimeException("Disconnected while in write operation");
                                }

                                if (newState == BluetoothGatt.STATE_DISCONNECTED) {
                                    Peripheral.this.gatt.close();
                                    Peripheral.this.gatt = null;
                                    Peripheral.this.wakeCommand(future, null);
                                }
                            });
                        }
                    });
                    if (!this.gatt.writeCharacteristic(characteristic)) {
                        throw new RuntimeException("Unable to write characteristic");
                    }
                });
            });
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<List<BluetoothGattService>> discoverServices() {
        SimpleFuture<List<BluetoothGattService>> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> {
                this.asyncWithFuture(future, () -> {
                    if (!this.connected) {
                        throw new NotConnectedException();
                    }

                    this.setCommandCallback(new CommandCallback() {
                        @Override
                        public void onServicesDiscovered(BluetoothGatt gatt, int status) {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new RuntimeException("Unable to discover services");
                            }

                            Peripheral.this.wakeCommand(future, gatt.getServices());
                        }
                        @Override
                        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (status != BluetoothGatt.GATT_SUCCESS) {
                                    throw new RuntimeException("Disconnected while discovering services");
                                }

                                if (newState == BluetoothGatt.STATE_DISCONNECTED) {
                                    Peripheral.this.gatt.close();
                                    Peripheral.this.gatt = null;
                                    Peripheral.this.wakeCommand(future, null);
                                }
                            });
                        }
                    });
                    if (!this.gatt.discoverServices()) {
                        throw new RuntimeException("Unable to discover services");
                    }
                });
            });
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<Void> setCharacteristicNotification(UUID uuid, boolean enable) {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> {
                this.asyncWithFuture(future, () -> {
                    if (!this.connected) {
                        throw new NotConnectedException();
                    }

                    BluetoothGattCharacteristic characteristic = this.getCharacteristicByUuid(uuid);
                    if (!this.gatt.setCharacteristicNotification(characteristic, enable)) {
                        throw new RuntimeException("Unable to set characteristic notification");
                    }

                    BluetoothGattDescriptor descriptor = characteristic.getDescriptor(CLIENT_CHARACTERISTIC_CONFIGURATION_DESCRIPTOR);
                    descriptor.setValue(enable ? BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE : BluetoothGattDescriptor.DISABLE_NOTIFICATION_VALUE);
                    if (!this.gatt.writeDescriptor(descriptor)) {
                        throw new RuntimeException("Unable to write client characteristic configuration descriptor");
                    }

                    this.setCommandCallback(new CommandCallback() {
                        @Override
                        public void onDescriptorWrite(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (status != BluetoothGatt.GATT_SUCCESS) {
                                    throw new RuntimeException("Unable to write descriptor");
                                }

                                if (!descriptor.getUuid().equals(CLIENT_CHARACTERISTIC_CONFIGURATION_DESCRIPTOR) || !descriptor.getCharacteristic().getUuid().equals(uuid)) {
                                    throw new UnexpectedCharacteristicException();
                                }

                                Peripheral.this.wakeCommand(future, null);
                            });
                        }
                        @Override
                        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (status != BluetoothGatt.GATT_SUCCESS) {
                                    throw new RuntimeException("Disconnected while setting characteristic notification");
                                }

                                if (newState == BluetoothGatt.STATE_DISCONNECTED) {
                                    Peripheral.this.gatt.close();
                                    Peripheral.this.gatt = null;
                                    Peripheral.this.wakeCommand(future, null);
                                }
                            });
                        }
                    });
                });
            });
        }
        return future;
    }

    public Stream<BluetoothGattCharacteristic> getNotifications() {
        QueueStream<BluetoothGattCharacteristic> stream = new QueueStream<>();
        synchronized (this) {
            this.notificationStreams.add(new WeakReference<>(stream));
        }
        return stream;
    }

    @SuppressLint("MissingPermission")
    public Future<byte[]> readDescriptor(UUID characteristic, UUID uuid) {
        SimpleFuture<byte[]> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> {
                this.asyncWithFuture(future, () -> {
                    if (!this.connected) {
                        throw new NotConnectedException();
                    }

                    BluetoothGattDescriptor descriptor = this.getDescriptorByUuid(characteristic, uuid);
                    this.setCommandCallback(new CommandCallback() {
                        @Override
                        public void onDescriptorRead(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (!descriptor.getUuid().equals(uuid)) {
                                    throw new UnexpectedCharacteristicException();
                                }

                                Peripheral.this.wakeCommand(future, descriptor.getValue());
                            });
                        }
                    });
                    if (!this.gatt.readDescriptor(descriptor)) {
                        throw new RuntimeException("Unable to read descriptor");
                    }
                });
            });
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<Void> writeDescriptor(UUID characteristic, UUID uuid, byte[] data, int writeType) {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> {
                this.asyncWithFuture(future, () -> {
                    if (!this.connected) {
                        throw new NotConnectedException();
                    }

                    BluetoothGattDescriptor descriptor = this.getDescriptorByUuid(characteristic, uuid);
                    descriptor.setValue(data);
                    this.setCommandCallback(new CommandCallback() {
                        @Override
                        public void onDescriptorWrite(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (!descriptor.getUuid().equals(uuid)) {
                                    throw new UnexpectedCharacteristicException();
                                }

                                Peripheral.this.wakeCommand(future, null);
                            });
                        }
                    });
                    if (!this.gatt.writeDescriptor(descriptor)) {
                        throw new RuntimeException("Unable to read characteristic");
                    }
                });
            });
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    private List<BluetoothGattCharacteristic> getCharacteristics() {
        List<BluetoothGattCharacteristic> result = new ArrayList<>();
        if (this.gatt != null) {
            for (BluetoothGattService service : this.gatt.getServices()) {
                result.addAll(service.getCharacteristics());
            }
        }
        return result;
    }

    @SuppressLint("MissingPermission")
    private BluetoothGattCharacteristic getCharacteristicByUuid(UUID uuid) {
        for (BluetoothGattCharacteristic characteristic : this.getCharacteristics()) {
            if (characteristic.getUuid().equals(uuid)) {
                return characteristic;
            }
        }

        throw new NoSuchCharacteristicException();
    }

    @SuppressLint("MissingPermission")
    private BluetoothGattDescriptor getDescriptorByUuid(UUID characteristicUuid, UUID uuid) {
        BluetoothGattCharacteristic characteristic = getCharacteristicByUuid(characteristicUuid);
        for (BluetoothGattDescriptor descriptor : characteristic.getDescriptors()) {
            if (descriptor.getUuid().equals(uuid)) {
                return descriptor;
            }
        }

        throw new NoSuchCharacteristicException();
    }

    private void queueCommand(Runnable callback) {
        if (this.executingCommand) {
            this.commandQueue.add(callback);
        } else {
            this.executingCommand = true;
            callback.run();
        }
    }

    private void setCommandCallback(CommandCallback callback) {
        assert this.commandCallback == null;
        this.commandCallback = callback;
    }

    private void runNextCommand() {
        assert this.executingCommand;
        this.commandCallback = null;
        if (this.commandQueue.isEmpty()) {
            this.executingCommand = false;
        } else {
            Runnable callback = this.commandQueue.remove();
            callback.run();
        }
    }

    private <T> void wakeCommand(SimpleFuture<T> future, T result) {
        future.wake(result);
        this.runNextCommand();
    }

    private <T> void asyncWithFuture(SimpleFuture<T> future, Runnable callback) {
        try {
            callback.run();
        } catch (Throwable ex) {
            future.wakeWithThrowable(ex);
            this.runNextCommand();
        }
    }

    private class Callback extends BluetoothGattCallback {
        @Override
        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
            synchronized (Peripheral.this) {
                switch (newState) {
                    case BluetoothGatt.STATE_CONNECTED:
                        Peripheral.this.connected = true;
                        break;
                    case BluetoothGatt.STATE_DISCONNECTED:
                        Peripheral.this.connected = false;
                        break;
                }
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onConnectionStateChange(gatt, status, newState);
                }
            }
            switch (newState) {
                case BluetoothGatt.STATE_CONNECTED:
                    Peripheral.this.adapter.onConnectionStateChanged(Peripheral.this.device.getAddress(), true);
                    break;
                case BluetoothGatt.STATE_DISCONNECTED:
                    Peripheral.this.adapter.onConnectionStateChanged(Peripheral.this.device.getAddress(), false);
                    break;
            }
        }

        @Override
        public void onCharacteristicRead(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onCharacteristicRead(gatt, characteristic, status);
                }
            }
        }

        @Override
        public void onCharacteristicWrite(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onCharacteristicWrite(gatt, characteristic, status);
                }
            }
        }

        @Override
        public void onServicesDiscovered(BluetoothGatt gatt, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onServicesDiscovered(gatt, status);
                }
            }
        }

        @Override
        public void onCharacteristicChanged(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic) {
            BluetoothGattCharacteristic characteristic2 = new BluetoothGattCharacteristic(characteristic.getUuid(), characteristic.getProperties(), characteristic.getPermissions());
            characteristic2.setValue(characteristic.getValue());
            synchronized (Peripheral.this) {
                for (WeakReference<QueueStream<BluetoothGattCharacteristic>> ref : Peripheral.this.notificationStreams) {
                    QueueStream<BluetoothGattCharacteristic> stream = ref.get();
                    if (stream != null) {
                        stream.add(characteristic2);
                    }
                }
            }
        }

        @Override
        public void onDescriptorWrite(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onDescriptorWrite(gatt, descriptor, status);
                }
            }
        }
    }

    private static abstract class CommandCallback extends BluetoothGattCallback {
        @Override
        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onCharacteristicRead(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onCharacteristicWrite(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onDescriptorRead(BluetoothGatt gatt, BluetoothGattDescriptor descriptor,
                                     int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onServicesDiscovered(BluetoothGatt gatt, int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onDescriptorWrite(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
            throw new UnexpectedCallbackException();
        }
    }
}
