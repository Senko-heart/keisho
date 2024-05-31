#![doc = include_str!("../README.md")]
#![no_std]
#![feature(marker_trait_attr)]
#![feature(freeze)]

use core::any::TypeId;
use core::fmt;
use core::fmt::Debug;
use core::marker::Freeze;
use core::marker::PhantomData;
use core::mem;
use core::mem::MaybeUninit;
use core::ops::Deref;
use core::ops::DerefMut;
use core::ptr::NonNull;

pub type Pointed<T> = <T as Deref>::Target;

pub unsafe trait ObjectPtr:
    Deref<Target: Hierarchy + Sized> + SameObjectPtr<Self> + Sized
{
    unsafe fn from_raw(ptr: NonNull<Pointed<Self>>) -> Self;
    fn into_raw(self) -> NonNull<Pointed<Self>>;
}

pub unsafe trait SameObjectPtr<U> {}

unsafe impl<T: Hierarchy> ObjectPtr for &T {
    unsafe fn from_raw(ptr: NonNull<Pointed<Self>>) -> Self {
        ptr.as_ref()
    }

    fn into_raw(self) -> NonNull<Pointed<Self>> {
        NonNull::from(self)
    }
}

unsafe impl<'a, T, U> SameObjectPtr<&'a U> for &'a T {}

unsafe impl<T: Hierarchy> ObjectPtr for &mut T {
    unsafe fn from_raw(mut ptr: NonNull<Pointed<Self>>) -> Self {
        ptr.as_mut()
    }

    fn into_raw(self) -> NonNull<Pointed<Self>> {
        NonNull::from(self)
    }
}

unsafe impl<'a, T, U> SameObjectPtr<&'a mut U> for &'a mut T {}

#[marker]
pub unsafe trait Upcastable<Target: ?Sized> {}

unsafe impl<T: DerefMut> Upcastable<Pointed<T>> for T {}
unsafe impl<T: DerefMut, U> Upcastable<U> for T where Pointed<T>: Upcastable<U> {}
unsafe impl Upcastable<Object> for Object {}

fn type_id<T>() -> TypeId {
    fn tid<'a, T: 'a>(_: PhantomData<&'static T>) -> TypeId {
        TypeId::of::<T>()
    }

    let f = tid::<T> as *const ();
    let f: fn(PhantomData<&T>) -> TypeId = unsafe { mem::transmute(f) };
    f(PhantomData)
}

pub struct ClassInfo {
    depth: u16,
    downable: fn(TypeId) -> bool,
    vtable: NonNull<fn(Void)>,
}

enum Void {}

pub unsafe trait Hierarchy: Virtual {
    const INFO: ClassInfo;
}

unsafe impl Hierarchy for Object {
    const INFO: ClassInfo = ClassInfo {
        depth: 0,
        downable: |_| false,
        vtable: unsafe {
            NonNull::new_unchecked(core::ptr::from_ref(&Self::TABLE).cast_mut()).cast::<fn(Void)>()
        },
    };
}

unsafe impl<T: DerefMut<Target: Hierarchy> + Upcastable<Object> + Virtual> Hierarchy for T {
    const INFO: ClassInfo = ClassInfo {
        depth: Pointed::<T>::INFO.depth + 1,
        downable: |id| type_id::<Self>() == id || (Pointed::<T>::INFO.downable)(id),
        vtable: unsafe {
            NonNull::new_unchecked(core::ptr::from_ref(&Self::TABLE).cast_mut()).cast::<fn(Void)>()
        },
    };
}

pub trait Virtual: VirtualDeref {
    type Dyn: ?Sized;
    const TABLE: Self::VTable;
}

#[repr(C)]
pub struct VList<V: Virtual, R>(fn(NonNull<V>) -> NonNull<V::Dyn>, R);

impl<V: Virtual, R: Copy> Copy for VList<V, R> {}
impl<V: Virtual, R: Clone> Clone for VList<V, R> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1.clone())
    }
}

pub unsafe trait VirtualDeref {
    type VTable: Copy + Freeze;
}

unsafe impl VirtualDeref for Object {
    type VTable = fn(NonNull<Object>) -> NonNull<dyn VirtualStub>;
}

unsafe impl<V: Virtual + Deref<Target: VirtualDeref>> VirtualDeref for V {
    type VTable = VList<V, <Pointed<V> as VirtualDeref>::VTable>;
}

#[repr(C)]
pub union Vt<VRoot: Virtual> {
    table: VRoot::VTable,
    array: [MaybeUninit<fn(Void)>; 1 << 16],
}

#[doc(hidden)]
pub const fn vsize<V: VirtualDeref>() -> usize {
    mem::size_of::<V::VTable>() / mem::size_of::<fn(Void)>()
}

#[rustfmt::skip]
impl<VRoot: Virtual<VTable = VList<VRoot, <Pointed<VRoot> as VirtualDeref>::VTable>> + Deref<Target: Virtual>> Vt<VRoot> {
    pub const unsafe fn new(f: fn(NonNull<VRoot>) -> NonNull<VRoot::Dyn>) -> Self {
        Self { table: VList(f, <Pointed<VRoot> as Virtual>::TABLE) }
    }

    pub const unsafe fn r#override<V: Virtual>(mut self, f: fn(NonNull<VRoot>) -> NonNull<V::Dyn>) -> Self {
        unsafe {
            let len = vsize::<VRoot>();
            let sub = vsize::<V>();
            self.array[len - sub] = core::mem::transmute(f);
        }
        self
    }

    pub const fn into_inner(self) -> VRoot::VTable {
        unsafe { self.table }
    }
}

#[macro_export]
macro_rules! vt {
    () => {
        unsafe { $crate::Vt::<Self>::new(|this| this) }.into_inner()
    };
    (override: $($T:ty),+ $(,)?) => {
        {
            let mut vt = unsafe { $crate::Vt::<Self>::new(|this| this) };
            $(
                vt = unsafe { vt.r#override::<$T>(|this| this) };
            )+
            vt.into_inner()
        }
    };
}

pub trait VirtualStub {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Object;

impl VirtualStub for Object {}

impl Virtual for Object {
    type Dyn = dyn VirtualStub;
    const TABLE: Self::VTable = |this| this;
}

pub struct Handle<P: ObjectPtr> {
    ptr: NonNull<Pointed<P>>,
    info: &'static ClassInfo,
}

impl<P: ObjectPtr> From<P> for Handle<P> {
    fn from(ptr: P) -> Self {
        Self {
            ptr: ptr.into_raw(),
            info: &Pointed::<P>::INFO,
        }
    }
}

impl<P: ObjectPtr<Target: Debug>> Debug for Handle<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (**self).fmt(f)
    }
}

impl<P: ObjectPtr> Drop for Handle<P> {
    fn drop(&mut self) {
        unsafe { P::from_raw(self.ptr) };
    }
}

impl<P: ObjectPtr> Deref for Handle<P> {
    type Target = Pointed<P>;

    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<P: ObjectPtr + DerefMut> DerefMut for Handle<P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

type TableFn<P> = fn(NonNull<Pointed<P>>) -> NonNull<<Pointed<P> as Virtual>::Dyn>;

impl<P: ObjectPtr> Handle<P> {
    pub fn upcast<Q: ObjectPtr + SameObjectPtr<P>>(self) -> Handle<Q>
    where
        Pointed<P>: Upcastable<Pointed<Q>>,
        Pointed<Q>: Upcastable<Object>,
    {
        Handle {
            ptr: self.ptr.cast(),
            info: self.info,
        }
    }

    pub fn downcast<Q: ObjectPtr + SameObjectPtr<P>>(self) -> Result<Handle<Q>, Self>
    where
        Pointed<Q>: Upcastable<Pointed<P>>,
        Pointed<P>: Upcastable<Object>,
    {
        if (self.info.downable)(type_id::<Pointed<Q>>()) {
            Ok(Handle {
                ptr: self.ptr.cast(),
                info: self.info,
            })
        } else {
            Err(self)
        }
    }

    pub fn downcast_ref<U>(&self) -> Option<Handle<&U>>
    where
        U: Upcastable<Pointed<P>> + Hierarchy,
        Pointed<P>: Upcastable<Object>,
    {
        if (self.info.downable)(type_id::<U>()) {
            Some(Handle {
                ptr: self.ptr.cast(),
                info: self.info,
            })
        } else {
            None
        }
    }

    pub fn downcast_mut<U>(&mut self) -> Option<Handle<&mut U>>
    where
        P: DerefMut,
        U: Upcastable<Pointed<P>> + Hierarchy,
        Pointed<P>: Upcastable<Object> + DerefMut,
    {
        if (self.info.downable)(type_id::<U>()) {
            Some(Handle {
                ptr: self.ptr.cast(),
                info: self.info,
            })
        } else {
            None
        }
    }

    fn map_to_virtual(&self) -> TableFn<P> {
        let offset = usize::from(self.info.depth - Pointed::<P>::INFO.depth);
        let vtable = self.info.vtable.as_ptr().cast::<TableFn<P>>();
        unsafe { vtable.add(offset).read() }
    }

    pub fn r#virtual(&self) -> &<Pointed<P> as Virtual>::Dyn {
        unsafe { (self.map_to_virtual())(self.ptr).as_ref() }
    }

    pub fn virtual_mut(&mut self) -> &mut <Pointed<P> as Virtual>::Dyn
    where
        P: DerefMut,
    {
        unsafe { (self.map_to_virtual())(self.ptr).as_mut() }
    }
}
