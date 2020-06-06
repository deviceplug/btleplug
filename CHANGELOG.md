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
