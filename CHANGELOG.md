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
