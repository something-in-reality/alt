//! Virtual displays, through CoreGraphics' private CGVirtualDisplay classes.

use dispatch2::DispatchQueue;
use objc2::{
    AnyThread, extern_class, extern_methods,
    rc::{Allocated, Retained},
    runtime::{AnyClass, Bool, NSObject},
};
use objc2_core_foundation::{CFArray, CFBoolean, CFDictionary, CFRetained, CGSize};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayCopyAllDisplayModes, CGDisplayMode,
    kCGDisplayShowDuplicateLowResolutionModes,
};
use objc2_foundation::{NSArray, NSString};

use super::Error;

extern_class!(
    #[unsafe(super(NSObject))]
    #[derive(Debug, PartialEq, Eq, Hash)]
    struct CGVirtualDisplayDescriptor;
);

extern_class!(
    #[unsafe(super(NSObject))]
    #[derive(Debug, PartialEq, Eq, Hash)]
    struct CGVirtualDisplayMode;
);

extern_class!(
    #[unsafe(super(NSObject))]
    #[derive(Debug, PartialEq, Eq, Hash)]
    struct CGVirtualDisplaySettings;
);

extern_class!(
    #[unsafe(super(NSObject))]
    #[derive(Debug, PartialEq, Eq, Hash)]
    struct CGVirtualDisplay;
);

impl CGVirtualDisplayDescriptor {
    extern_methods!(
        #[unsafe(method(new))]
        fn new() -> Retained<Self>;
        #[unsafe(method(setQueue:))]
        fn set_queue(&self, queue: &DispatchQueue);
        #[unsafe(method(setName:))]
        fn set_name(&self, name: &NSString);
        #[unsafe(method(setMaxPixelsWide:))]
        fn set_max_pixels_wide(&self, pixels: u32);
        #[unsafe(method(setMaxPixelsHigh:))]
        fn set_max_pixels_high(&self, pixels: u32);
        #[unsafe(method(setSizeInMillimeters:))]
        fn set_size_in_millimeters(&self, size: CGSize);
        #[unsafe(method(setVendorID:))]
        fn set_vendor_id(&self, id: u32);
        #[unsafe(method(setProductID:))]
        fn set_product_id(&self, id: u32);
        #[unsafe(method(setSerialNum:))]
        fn set_serial_num(&self, serial: u32);
    );
}

impl CGVirtualDisplayMode {
    extern_methods!(
        #[unsafe(method(initWithWidth:height:refreshRate:))]
        fn init(this: Allocated<Self>, width: u32, height: u32, rate: f64) -> Retained<Self>;
    );
}

impl CGVirtualDisplaySettings {
    extern_methods!(
        #[unsafe(method(new))]
        fn new() -> Retained<Self>;
        #[unsafe(method(setModes:))]
        fn set_modes(&self, modes: &NSArray<CGVirtualDisplayMode>);
        #[unsafe(method(setHiDPI:))]
        fn set_hidpi(&self, hidpi: u32);
    );
}

impl CGVirtualDisplay {
    extern_methods!(
        #[unsafe(method(initWithDescriptor:))]
        fn init(
            this: Allocated<Self>,
            descriptor: &CGVirtualDisplayDescriptor,
        ) -> Option<Retained<Self>>;
        #[unsafe(method(applySettings:))]
        fn apply_settings(&self, settings: &CGVirtualDisplaySettings) -> Bool;
        #[unsafe(method(displayID))]
        fn display_id(&self) -> CGDirectDisplayID;
    );
}

/// The classes a virtual display uses, which the system provides at runtime.
const CLASSES: [&std::ffi::CStr; 4] = [
    c"CGVirtualDisplayDescriptor",
    c"CGVirtualDisplayMode",
    c"CGVirtualDisplaySettings",
    c"CGVirtualDisplay",
];

/// What a virtual display presents to the system: a name, the identity macOS keys its saved
/// settings by, a physical size, and one HiDPI mode of `width` × `height` points, which renders
/// at twice that in pixels.
pub struct Descriptor {
    pub name: String,
    pub vendor: u32,
    pub product: u32,
    pub serial: u32,
    pub millimeters: CGSize,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: f64,
}

/// A virtual display, which exists while this value is held.
pub struct VirtualDisplay {
    display: Retained<CGVirtualDisplay>,
    width: u32,
    height: u32,
}

// SAFETY: The value only retains and releases the object, and reads its display ID, which never
// changes; reference counting is atomic.
unsafe impl Send for VirtualDisplay {}

impl VirtualDisplay {
    /// Creates the display described by `descriptor`; the system calls back on `queue`.
    pub fn new(descriptor: &Descriptor, queue: &DispatchQueue) -> Result<Self, Error> {
        if let Some(missing) = CLASSES.iter().find(|name| AnyClass::get(name).is_none()) {
            return Err(Error::NoClass(missing.to_string_lossy().into_owned()));
        }
        let refused = |step| Error::VirtualDisplay {
            name: descriptor.name.clone(),
            step,
        };
        let pixels = |points: u32| points.checked_mul(2).ok_or_else(|| refused("sizing"));
        let native = CGVirtualDisplayDescriptor::new();
        native.set_queue(queue);
        native.set_name(&NSString::from_str(&descriptor.name));
        native.set_max_pixels_wide(pixels(descriptor.width)?);
        native.set_max_pixels_high(pixels(descriptor.height)?);
        native.set_size_in_millimeters(descriptor.millimeters);
        native.set_vendor_id(descriptor.vendor);
        native.set_product_id(descriptor.product);
        native.set_serial_num(descriptor.serial);
        let display = CGVirtualDisplay::init(CGVirtualDisplay::alloc(), &native)
            .ok_or_else(|| refused("creating"))?;

        let mode = CGVirtualDisplayMode::init(
            CGVirtualDisplayMode::alloc(),
            descriptor.width,
            descriptor.height,
            descriptor.refresh_rate,
        );
        let settings = CGVirtualDisplaySettings::new();
        settings.set_hidpi(1);
        settings.set_modes(&NSArray::from_slice(&[&*mode]));
        if !display.apply_settings(&settings).as_bool() {
            return Err(refused("applying modes to"));
        }
        Ok(Self {
            display,
            width: descriptor.width,
            height: descriptor.height,
        })
    }

    pub fn id(&self) -> CGDirectDisplayID {
        self.display.display_id()
    }

    /// The HiDPI mode, which macOS leaves unselected: a new display starts in its largest mode, at
    /// one pixel per point.
    pub fn mode(&self) -> Result<CFRetained<CGDisplayMode>, Error> {
        let options = CFDictionary::from_slices(
            &[unsafe { kCGDisplayShowDuplicateLowResolutionModes }],
            &[CFBoolean::new(true)],
        );
        // SAFETY: The options are a dictionary of CFString keys and CFBoolean values.
        let modes = unsafe { CGDisplayCopyAllDisplayModes(self.id(), Some(options.as_opaque())) }
            .ok_or(Error::Unavailable("the display's modes"))?;
        // SAFETY: The call returns an array of display modes.
        let modes = unsafe { CFRetained::cast_unchecked::<CFArray<CGDisplayMode>>(modes) };
        let (width, height) = (self.width as usize, self.height as usize);
        modes
            .iter()
            .find(|mode| {
                let mode = Some(&**mode);
                CGDisplayMode::width(mode) == width
                    && CGDisplayMode::height(mode) == height
                    && CGDisplayMode::pixel_width(mode) == 2 * width
                    && CGDisplayMode::pixel_height(mode) == 2 * height
            })
            .ok_or(Error::Unavailable("the HiDPI mode"))
    }
}
