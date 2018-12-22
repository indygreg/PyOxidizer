use libc::c_int;

#[cfg_attr(windows, link(name="pythonXY"))] extern "C" {
    #[cfg(all(unix, Py_3_7))]
    pub fn PyOS_BeforeFork() -> ();
    #[cfg(all(unix, Py_3_7))]
    pub fn PyOS_AfterFork_Parent() -> ();
    #[cfg(all(unix, Py_3_7))]
    pub fn PyOS_AfterFork_Child() -> ();

    pub fn PyOS_InterruptOccurred() -> c_int;
    pub fn PyOS_InitInterrupts() -> ();
    pub fn PyOS_AfterFork() -> ();
}

