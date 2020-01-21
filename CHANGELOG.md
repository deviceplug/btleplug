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
