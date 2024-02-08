// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Register Debug Support Infrastructure
//!
//! This module provides optional infrastructure to query debug information from
//! register types implementing the [`RegisterDebugInfo`] trait. This
//! information can then be used by the [`RegisterDebugValue`] type to produce a
//! human-readable representation of a register's fields and values.

use core::fmt;
use core::marker::PhantomData;

use crate::{
    fields::{Field, TryFromValue},
    RegisterLongName, UIntLike,
};

/// `FieldValueEnumSeq` is a debug helper trait representing a sequence of
/// [field enum types](crate::fields::Field::read_as_enum). It provides methods
/// to recurse through this sequence of types and thus call methods on them,
/// such as [`try_from_value`](crate::fields::TryFromValue::try_from_value).
///
/// Its primary use lies in the [`RegisterDebugInfo`] trait. This trait provides
/// facilities useful for providing information on the layout of a register
/// (such as, which fields it has and which known values those fields can
/// assume). Such information is usually only available at compile time. This
/// trait makes runtime-representable data available at runtime. However,
/// information encoded solely in types can't simply be used at runtime, which
/// is where this trait comes in.
pub trait FieldValueEnumSeq<U: UIntLike> {
    /// Iterates over the sequence of types and performs the following steps:
    ///
    /// 1. Invokes the `data` function argument. This is expected to provide the
    ///    numeric (`UIntLike`) value of the register field that the current type
    ///    corresponds to.
    ///
    /// 2. Invoke [`try_from_value`](crate::fields::TryFromValue::try_from_value)
    ///    on the current [field enum type](crate::fields::Field::read_as_enum),
    ///    passing the value returned by `data`.
    ///
    /// 3. Provide the returned value to the `f` function argument. This is
    ///    either the enum representation (if the field value belongs to a known
    ///    variant, or the numeric field value returned by `data`.
    ///
    /// In practice, this method should be used to iterate over types and other
    /// runtime-accessible information in tandem, to produce a human-readable
    /// register dump.
    ///
    /// Importantly, `data` is invoked for every type in the sequence, and
    /// every invocation of `data` is followed by a single invocation of `f`.
    fn recurse_try_from_value(data: &mut impl FnMut() -> U, f: &mut impl FnMut(&dyn fmt::Debug));
}

/// End-of-list type for the [`FieldValueEnumSeq`] sequence.
pub enum FieldValueEnumNil {}
impl<U: UIntLike> FieldValueEnumSeq<U> for FieldValueEnumNil {
    fn recurse_try_from_value(_data: &mut impl FnMut() -> U, _f: &mut impl FnMut(&dyn fmt::Debug)) {
    }
}

/// List element for the [`FieldValueEnumSeq`] sequence.
pub enum FieldValueEnumCons<
    U: UIntLike,
    H: TryFromValue<U, EnumType = H> + fmt::Debug,
    T: FieldValueEnumSeq<U>,
> {
    // This variant can never be constructed, as `Infallible` can't be:
    _Impossible(
        core::convert::Infallible,
        PhantomData<U>,
        PhantomData<H>,
        PhantomData<T>,
    ),
}
impl<U: UIntLike, H: TryFromValue<U, EnumType = H> + fmt::Debug, T: FieldValueEnumSeq<U>>
    FieldValueEnumSeq<U> for FieldValueEnumCons<U, H, T>
{
    fn recurse_try_from_value(data: &mut impl FnMut() -> U, f: &mut impl FnMut(&dyn fmt::Debug)) {
        // Query debug information from first type, then recurse into the next.

        // It's imprtant that we call `data` _exactly_ once here.
        let extracted_value = data();

        // It's important that we call `f` _exactly_ once here.
        match H::try_from_value(extracted_value) {
            Some(v) => f(&v),
            None => f(&extracted_value),
        }

        // Continue the recursion:
        T::recurse_try_from_value(data, f)
    }
}

/// [`RegisterDebugInfo`] exposes debugging information from register types.
///
/// The exposed information is composed of both types (such as the individual
/// [field enum types](crate::fields::Field::read_as_enum) generated by the
/// [`crate::register_bitfields`] macro), as well as runtime-queryable
/// information in the form of data.
///
/// Where applicable, the index of type information and runtime-queryable data
/// match. For instance, the `i`th element of the
/// [`RegisterDebugInfo::FieldValueEnumTypes`] associated type sequence
/// corresponds to the `i`th element of the array returned by the
/// [`RegisterDebugInfo::field_names`] method.
pub trait RegisterDebugInfo<T: UIntLike>: RegisterLongName {
    /// Associated type representing a sequence of all field-value enum types of
    /// this [`RegisterLongName`] register.
    ///
    /// See [`FieldValueEnumSeq`]. The index of types in this sequence
    /// correspond to indices of values returned from the [`fields`] and
    /// [`field_names`] methods.
    ///
    /// [`field_names`]: RegisterDebugInfo::field_names
    /// [`fields`]: RegisterDebugInfo::fields
    type FieldValueEnumTypes: FieldValueEnumSeq<T>;

    /// The name of the register.
    fn name() -> &'static str;

    /// The names of the fields in the register.
    ///
    /// The length of the returned slice is identical to the length of the
    /// [`FieldValueEnumTypes`] sequence and [`fields`] return value. For every
    /// index `i`, the element of this slice corresponds to the type at the
    /// `i`th position in the [`FieldValueEnumTypes`] sequence and element at
    /// the `i`th position in the [`fields`] return value.
    ///
    /// [`FieldValueEnumTypes`]: RegisterDebugInfo::FieldValueEnumTypes
    /// [`fields`]: RegisterDebugInfo::fields
    fn field_names() -> &'static [&'static str];

    /// The fields of a register.
    ///
    /// The length of the returned slice is identical to the length of the
    /// [`FieldValueEnumTypes`] sequence and [`field_names`] return value. For
    /// every index `i`, the element of this slice corresponds to the type at
    /// the `i`th position in the [`FieldValueEnumTypes`] sequence and element
    /// at the `i`th position in the [`field_names`] return value.
    ///
    /// [`FieldValueEnumTypes`]: RegisterDebugInfo::FieldValueEnumTypes
    /// [`field_names`]: RegisterDebugInfo::field_names
    fn fields() -> &'static [Field<T, Self>]
    where
        Self: Sized;
}

/// `RegisterDebugValue` captures a register's value and implements
/// [`fmt::Debug`] to provide a human-readable representation of the register
/// state.
///
/// Its usage incurs the inclusion of additional data into the final binary,
/// such as the names of all register fields and defined field value variants
/// (see [`crate::fields::Field::read_as_enum`]).
///
/// This type contains a local copy of the register value used for providing
/// debug information. It will not access the actual backing register.
pub struct RegisterDebugValue<T, E>
where
    T: UIntLike,
    E: RegisterDebugInfo<T>,
{
    pub(crate) data: T,
    pub(crate) _reg: core::marker::PhantomData<E>,
}

impl<T, E> fmt::Debug for RegisterDebugValue<T, E>
where
    T: UIntLike + 'static,
    E: RegisterDebugInfo<T>,
    E: 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // This is using the core library's formatting facilities to produce an
        // output similar to Rust's own derive-Debug implementation on structs.
        //
        // We start by printing the struct's name and opening braces:
        let mut debug_struct = f.debug_struct(E::name());

        // Now, obtain iterators over both the struct's field types and
        // names. They are guaranteed to match up:
        let mut names = E::field_names().iter();
        let mut fields = E::fields().iter();

        // To actually resolve the field's known values (encoded in the field
        // enum type's variants), we need to recurse through those field
        // types. Their ordering is guaranteed to match up with the above
        // calls. For more information on what these closures do and how they
        // are invoked, consult the documentation of `recurse_try_from_value`.
        let mut data = || fields.next().unwrap().read(self.data);
        let mut debug_field = |f: &dyn fmt::Debug| {
            debug_struct.field(names.next().unwrap(), f);
        };

        // Finally, recurse through all the fields:
        E::FieldValueEnumTypes::recurse_try_from_value(&mut data, &mut debug_field);

        debug_struct.finish()
    }
}