//! Finder file promises run on a background operation queue after an accepted drop.
use geezipx_egui::export::DeferredExport;
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol, ProtocolObject},
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::*;
use objc2_foundation::{
    NSArray, NSData, NSError, NSOperationQueue, NSPoint, NSRect, NSString, NSURL,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

type Finished = Box<dyn Fn(Result<bool, String>) + Send>;
struct Completion {
    pending: AtomicUsize,
    finished: Mutex<Option<Finished>>,
    cancel: Arc<AtomicBool>,
}
impl Completion {
    fn finish(&self, result: Result<bool, String>) {
        if let Some(finished) = self
            .finished
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            finished(result);
        }
    }
}
struct PromiseState {
    export: Arc<DeferredExport>,
    name: String,
    completion: Arc<Completion>,
    queue: Retained<NSOperationQueue>,
}
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = objc2::AnyThread]
    #[name = "GeeZipXFilePromise"]
    #[ivars = PromiseState]
    struct Promise;
    unsafe impl NSObjectProtocol for Promise {}
    unsafe impl NSFilePromiseProviderDelegate for Promise {
        #[unsafe(method_id(filePromiseProvider:fileNameForType:))]
        fn file_name(&self, _: &NSFilePromiseProvider, _: &NSString) -> Retained<NSString> {
            NSString::from_str(&self.ivars().name)
        }
        #[unsafe(method(filePromiseProvider:writePromiseToURL:completionHandler:))]
        fn write(
            &self,
            _: &NSFilePromiseProvider,
            url: &NSURL,
            handler: &block2::DynBlock<dyn Fn(*mut NSError)>,
        ) {
            let state = self.ivars();
            state.export.confirm_drop();
            let result = if state.completion.cancel.load(Ordering::Relaxed) {
                Err("拖放已取消。".into())
            } else if let Some(path) = url.path() {
                state
                    .export
                    .write_promised_root(&state.name, &PathBuf::from(path.to_string()))
            } else {
                Err("拖放目标不是文件路径。".into())
            };
            match result {
                Ok(()) => {
                    handler.call((std::ptr::null_mut(),));
                    if state.completion.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
                        state.completion.finish(Ok(true));
                    }
                }
                Err(error) => {
                    // SAFETY: A concrete error domain/code with no userInfo pointers;
                    // the error is retained until the synchronous completion call returns.
                    let native = unsafe {
                        NSError::errorWithDomain_code_userInfo(
                            &NSString::from_str("GeeZipX.Export"),
                            1,
                            None,
                        )
                    };
                    handler.call((Retained::as_ptr(&native).cast_mut(),));
                    state.export.control.cancel();
                    state.completion.finish(Err(error));
                }
            }
        }
        #[unsafe(method_id(operationQueueForFilePromiseProvider:))]
        fn queue(&self, _: &NSFilePromiseProvider) -> Retained<NSOperationQueue> {
            self.ivars().queue.clone()
        }
    }
);
struct DragState {
    completion: Arc<Completion>,
}
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GeeZipXPromiseDragSource"]
    #[ivars = DragState]
    struct Source;
    unsafe impl NSObjectProtocol for Source {}
    unsafe impl NSDraggingSource for Source {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn mask(&self, _: &NSDraggingSession, _: NSDraggingContext) -> NSDragOperation {
            NSDragOperation::Copy
        }
        #[unsafe(method(draggingSession:endedAtPoint:operation:))]
        fn ended(&self, _: &NSDraggingSession, _: NSPoint, operation: NSDragOperation) {
            if operation == NSDragOperation::None {
                self.ivars().completion.finish(Ok(false));
            }
        }
    }
);
pub(super) fn start(
    frame: &eframe::Frame,
    export: Arc<DeferredExport>,
    finished: impl Fn(Result<bool, String>) + Send + 'static,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("拖放需要主线程")?;
    let RawWindowHandle::AppKit(handle) =
        frame.window_handle().map_err(|e| e.to_string())?.as_raw()
    else {
        return Err("AppKit 窗口不可用".into());
    };
    let roots: Vec<_> = export
        .items
        .iter()
        .filter(|item| !item.name.contains('/'))
        .collect();
    let completion = Arc::new(Completion {
        pending: AtomicUsize::new(roots.len()),
        finished: Mutex::new(Some(Box::new(finished))),
        cancel,
    });
    // SAFETY: eframe lends the live NSView on the main thread; all retained objects
    // passed to AppKit remain owned by its drag session and promise providers.
    unsafe {
        let view = &*handle.ns_view.as_ptr().cast::<NSView>();
        let window = view.window().ok_or("窗口不可用")?;
        let position = window.mouseLocationOutsideOfEventStream();
        let image = NSImage::initWithData(
            NSImage::alloc(),
            &NSData::with_bytes(include_bytes!("../../../assets/drag-file.png")),
        )
        .ok_or("拖放图标不可用")?;
        let rect = NSRect::new(position, image.size());
        let queue = NSOperationQueue::new();
        queue.setMaxConcurrentOperationCount(1);
        let mut items = Vec::new();
        for root in roots {
            let allocated = Promise::alloc().set_ivars(PromiseState {
                export: export.clone(),
                name: root.name.clone(),
                completion: completion.clone(),
                queue: queue.clone(),
            });
            let delegate: Retained<Promise> = msg_send![super(allocated), init];
            let provider = NSFilePromiseProvider::initWithFileType_delegate(
                NSFilePromiseProvider::alloc(),
                &NSString::from_str(if root.is_dir {
                    "public.folder"
                } else {
                    "public.data"
                }),
                ProtocolObject::from_ref(&*delegate),
            );
            // AppKit's delegate is weak: provider-owned userInfo keeps it alive for
            // promised writes that occur after the dragging session has ended.
            provider.setUserInfo(Some(&delegate));
            let item = NSDraggingItem::initWithPasteboardWriter(
                NSDraggingItem::alloc(),
                &ProtocolObject::from_retained(provider),
            );
            item.setDraggingFrame_contents(rect, Some(&*image));
            items.push(item);
        }
        let allocated = Source::alloc(mtm).set_ivars(DragState { completion });
        let source: Retained<Source> = msg_send![super(allocated), init];
        let timestamp = NSApp(mtm)
            .currentEvent()
            .map_or(0.0, |event| event.timestamp());
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(NSEventType::LeftMouseDragged, position, NSEventModifierFlags::empty(), timestamp, window.windowNumber(), None, 0, 1, 1.0).ok_or("无法创建拖放事件")?;
        view.beginDraggingSessionWithItems_event_source(
            &NSArray::from_retained_slice(&items),
            &event,
            ProtocolObject::from_ref(&*source),
        );
    }
    Ok(())
}
