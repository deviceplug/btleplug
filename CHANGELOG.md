# 0.10.2 (2022-10-30)

## Features

- Implement Display on PeripheralId

## Bugfixes

- Fix issues with panics on device disconnect on macOS

# 0.10.1 (2022-09-23)

## Features

- Add ability to disconnect devices on macOS/iOS

# 0.10.0 (2022-07-30)

## Features

- Add Android Support

## Breaking Changes

- Update to Uuid v1, which is incompatible with Uuid v0.x. This may cause issues in upgrades.

# 0.9.2 (2022-03-05)

## Features

- UWP (Windows) devices now disconnect on drop or calls to disconnect
- Improve characteristic finding resilience on UWP (Windows)

## Bugfixes

- Update to windows-rs 0.33
  - Should fix issues with COM casting panics in older versions of windows
- Fix panic when multiple discovery calls are made on corebluetooth (macOS)
- Update Dashmap version to resolve RUSTSEC-2022-0002

# 0.9.1 (2022-01-12)

## Features

- `BDAddr` and `PeripheralId` are now guaranteed to implement `Hash`, `Ord` and `PartialOrd` on all
  platforms.

## Bugfixes

- Linux implementation will now synthesise `DeviceConnected` events at the start of the event stream
  for all peripherals which were already connected at the point that the event stream was requested.
- `Central` methods on Linux will now correctly only affect the correct Bluetooth adapter, rather
  than all adapters on the system.
- Filters are now supported for macOS, allowing the library to work on macOS >= 12.

# 0.9.0 (2021-10-20)

## Features

- Added Received Signal Strength Indicator (RSSI) peripheral property.
- Peripheral `notifications()` streams can now be queried before any
  connection and remain valid independent of any re-connections.
- Characteristics are now grouped by service, and infomation about services is available. The old
  API to access characteristics without specifying a service UUIDs is retained for backwards
  compatibility.
- Better logging and other minor improvements in examples.
- Added method to get adapter information. For now it only works on Linux.

## Breaking changes

- Removed `CentralEvent::DeviceLost`. It wasn't emitted anywhere.
- Changed tx_power_level type from i8 to i16.
- Removed `PeripheralProperties::discovery_count`.
- New `PeripheralId` type is used as an opaque ID for peripherals, as the MAC address is not
  available on all platforms.
- Added optional `ScanFilter` parameter to `Central::start_scan` to filter by service UUIDs.

## Bugfixes

- `Peripheral::is_connected` now works on Mac OS, and works better on Windows.
- Fixed bug on Windows where RSSI was reported as TX power.
- Report address type on Windows.
- Report all advertised service UUIDs on Windows, rather than only those in the most recent
  advertisement.
- Fixed bug with service caching on Windows.
- Fixed bug with concurrent streams not working on Linux.

# 0.8.1 (2021-08-14)

## Bugfixes

- Errors now Sync/Send (usable with Anyhow)
- Characteristic properties now properly reported on windows

# 0.8.0 (2021-07-27)

## Features

- Overhaul API, moving to async based system

## Breaking Changes

- Pretty much everything? The whole API got rewritten. All hail the new flesh.

# 0.7.3 (2021-07-25)

## Bugfixes

- Fix issue with characteristic array not updating on Win10
- #172: Fix setting local_name in macOS

# 0.7.2 (2021-04-04)

## Bugfixes

- Windows UWP characteristic methods now return errors instead of unwrapping everything.

# 0.7.1 (2021-03-01)

## Bugfixes

- Fixed commit/merge issues with 0.7.0 that ended up with incorrect dependencies being brought in.

# 0.7.0 (2021-02-28) (Yanked)

## Breaking API Changes

- Move to using Uuid crate instead of having an internal type.
- Remove discover_characteristics_in_range (unused or duplicated elsewhere)
- write() commands are now passed a WriteType for specifying WriteWith/WithoutResponse
- Variants added to CentralEvent enum, may break exhaustive checks

## Features

- Add capabilities for service and manufacturer advertisements
- Lots of CoreBluetooth cleanup
- Update to using windows library (instead of winrt)
- Replace usage of async_std for channels in macOS with futures crate

## Bugfixes

- De-escalate log message levels, so there are less message spams at the info level.

# 0.6.0 (2021-02-04)

## Breaking API Changes

- Removed many _async methods that were unimplemented
- Stopped returning write values when not needed.

## Features

- Complete rewrite of Bluez core
  - Now uses DBus API instead of raw socket access
- Windows support moved to WinRT library
- Move from failure to thiserror for error handling
  - failure was deprecated a while ago

## Bugfixes

- Windows UWP no longer panics on scan when radio not connected.

# 0.5.5 (2021-01-18)

## Bugfixes

- Fix dependency issue with async-std channels

# 0.5.4 (2020-10-06)

## Bugfixes

- Fix issue where library panics whenever a characteristic is read instead of
  notified on macOS.

# 0.5.3 (2020-10-05)

## Bugfixes

- Fix issue where library panics whenever a characteristic is written without
  response on macOS.

# 0.5.2 (2020-10-04)

## Features

- UUID now takes simplified inputs for from_str()
- Read/Write added for CoreBluetooth
- Example improvements

## Bugfixes

- Windows UWP characteristics now actually reads on read(), instead of just
  returning []

## Bugfixes

# 0.5.1 (2020-08-03)

## Bugfixes

* Fixed issue with peripheral updates in adapter manager wiping out peripherals
  completely (#64)
* Ran rustfmt (misformatted code is a bug ok?)

# 0.5.0 (2020-07-26)

## Features

* Moved events from callbacks to channels (currently using std::channel, will
  change to future::Stream once we go async).
* Moved from using Arc<Mutex<HashMap<K,V>>> to Arc<DashMap<K,V>>. Slightly
  cleaner, less locking boilerplate.

## Bugfixes

* Centralized peripheral management into the AdapterManager class, which should
  clean up a bunch of bugs both filed and not.

# 0.4.4 (2020-07-22)

## Bugfixes

* Fix peripheral connect panic caused by uuid length on macOS (#43)
* Windows/macOS devices now emit events on device disconnect (#54)

# 0.4.3 (2020-06-05)

## Features

* Allow notification handlers to be FnMut
* Added new examples
* Update dependencies

## Bugfixes

* Fix local_name on macOS 10.15

# 0.4.2 (2020-04-18)

## Features

* Some types now capable of serde de/serialization, using "serde" feature
* Added new examples

## Bugfixes

* Adapters functions now return vectors of some kind of adapter on all
  platforms.
* Bluez notification handlers now live with the peripheral.
* Bluez defaults to active scan.
* Remove all println statements in library (mostly in the windows library),
  replace with log macros.

# 0.4.1 (2020-03-16)

## Features

* Get BDAddr and UUID from String
* More examples
* Update dependencies

# 0.4.0 (2020-01-20)

## Features

* Added CoreBluetooth Support, using async-std with most of the async
  parts wrapped in block_on calls for now. Library now supports
  Win10/MacOS/Linux/Maybe iOS.
* Brought code up to Rust 2018 standard
* Added Characteristic UUID to ValueNotification struct, since
  only linux deals with Start/End/Value handles

# 0.3.1 (2020-01-11)

## Features

* Initial fork from rumble
* Brought in winrt patch, as well as other PRs on that project.
