mod bindings;

pub struct MultiTouchSimulator {
    ptr: *mut bindings::MultiTouchSimulator,
}

impl MultiTouchSimulator {
    pub fn new(width: i32, height: i32) -> MultiTouchSimulator {
        Self {
            ptr: unsafe { bindings::create_simulator(width, height) },
        }
    }

    pub fn touch_down(&mut self, slot: i32, x: i32, y: i32, tracking_id: i32) {
        unsafe { bindings::touch_down(self.ptr, slot, x, y, tracking_id) }
    }

    pub fn touch_up(&mut self, slot: i32) {
        unsafe { bindings::touch_up(self.ptr, slot) }
    }

    pub fn touch_move(&mut self, slot: i32, x: i32, y: i32) {
        unsafe { bindings::touch_move(self.ptr, slot, x, y) }
    }
}

impl Drop for MultiTouchSimulator {
    fn drop(&mut self) {
        unsafe {
            bindings::destroy_simulator(self.ptr);
        }
    }
}
