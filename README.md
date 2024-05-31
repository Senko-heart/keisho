Single inheritance model classes with casts and virtual functions in Rust.

## Requires nightly

Necessary features: `marker_trait_attr`, `freeze`.

## Usage

Every class is an `Object` (i.e. is upcastable to `Object`).
Class consists of three parts: `struct`, `trait` and `impl Virtual`.

1. `struct` requires a `base` field it `DerefMut`s to. `base` must be a class too.
2. `trait` handles virtual dispatch. By convention, it's called `Virtual[ClassName]`.
3. `impl Virtual for [ClassName]` sets the `type Dyn = Virtual[ClassName]` and `const TABLE: Self::VTable = vt!()`.

To add overrides, you'd write `vt!(override: BaseA, BaseB, ...)` and implement their traits on your derived class.
There is no delegation at the moment, you'd have to override all methods of the chosen ancestor if you want to override any.

## Example

```rust
use keisho::*;
use derive_more::Deref;
use derive_more::DerefMut;

#[derive(Debug, Deref, DerefMut)]
pub struct Animal {
    base: Object,
}

pub trait VirtualAnimal {
    fn make_noise(&self);
}

impl VirtualAnimal for Animal {
    fn make_noise(&self) {
        println!("*noise*");
    }
}

impl Virtual for Animal {
    type Dyn = dyn VirtualAnimal;
    const TABLE: Self::VTable = vt!();
}

#[derive(Debug, Deref, DerefMut)]
pub struct Cat {
    #[deref]
    #[deref_mut]
    base: Animal,
    color: String,
}

impl VirtualStub for Cat {}

impl Virtual for Cat {
    type Dyn = dyn VirtualStub;
    const TABLE: Self::VTable = vt!();
}

#[derive(Debug, Deref, DerefMut)]
pub struct Dog {
    #[deref]
    #[deref_mut]
    base: Animal,
    size: usize,
}

impl VirtualStub for Dog {}

impl Virtual for Dog {
    type Dyn = dyn VirtualStub;
    const TABLE: Self::VTable = vt!();
}

#[derive(Debug, Deref, DerefMut)]
pub struct StrayCat {
    #[deref]
    #[deref_mut]
    base: Cat,
    neutered: bool,
}

impl VirtualStub for StrayCat {}

impl VirtualAnimal for StrayCat {
    fn make_noise(&self) {
        println!("😭");
    }
}

impl Virtual for StrayCat {
    type Dyn = dyn VirtualStub;
    const TABLE: Self::VTable = vt!(override: Animal);
}

fn main() {
    let mut value = StrayCat {
        base: Cat {
            base: Animal { base: Object },
            color: "Green".to_string(),
        },
        neutered: false,
    };
    
    // Handles store runtime object information on construction. Unwrapping a handle would lead to loss of information.
    let stray_cat = Handle::from(&mut value); // Starting pointer points to StrayCat, so its handle only knows this much.
    let object = stray_cat.upcast::<&mut Animal>(); // Upcasts never fail and happen between pointers of the same type.

    // Specialized downcasts aren't just a matter of convenience. Unlike deref, they can fail, and they don't have alternatives
    // that avoid consuming Handle.
    dbg!(object.downcast_ref::<Cat>()); // Some(Cat { ... })
    dbg!(object.downcast_ref::<Dog>()); // None
    object.make_noise(); // Non-virtual calls resolve statically (through trait impl on a type or its closest ancestor type).
    object.r#virtual().make_noise(); // You have to use .virtual(_mut) to be able to call virtual override.
}
```

## Limitations

- Fundamental: it is strictly single inheritance, it requires aforementioned nigthly features, and it is alien to Rust principles.
- Hard to solve: delegation (self.base.method(...)), mixins, QoL macros for code deduplication/simplification.
- Hard to solve, but not necessary: static typing of generic conversions can be more convenient and easy to understand.
- Solvable: API improvements (adding methods for `Box`/`Rc`/`Arc`/`Pin`).
