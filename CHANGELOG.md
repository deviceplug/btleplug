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
