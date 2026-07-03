pub use root::*;

const _: () = ::planus::check_version_compatibility("planus-1.3.0");

/// The root namespace
///
/// Generated from these locations:
/// * File `schema/asmux.fbs`
#[no_implicit_prelude]
#[allow(clippy::needless_lifetimes)]
mod root {
    /// The namespace `asmux`
    ///
    /// Generated from these locations:
    /// * File `schema/asmux.fbs`
    pub mod asmux {
        /// The namespace `asmux.wire`
        ///
        /// Generated from these locations:
        /// * File `schema/asmux.fbs`
        pub mod wire {
            /// The enum `AttachMode` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Enum `AttachMode` in the file `schema/asmux.fbs:15`
            #[derive(
                Copy,
                Clone,
                Debug,
                PartialEq,
                Eq,
                PartialOrd,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            #[repr(i8)]
            pub enum AttachMode {
                /// The variant `FromCursor` in the enum `AttachMode`
                FromCursor = 0,

                /// The variant `LiveOnly` in the enum `AttachMode`
                LiveOnly = 1,

                /// The variant `FromEarliest` in the enum `AttachMode`
                FromEarliest = 2,
            }

            impl AttachMode {
                /// Array containing all valid variants of AttachMode
                pub const ENUM_VALUES: [Self; 3] =
                    [Self::FromCursor, Self::LiveOnly, Self::FromEarliest];
            }

            impl ::core::convert::TryFrom<i8> for AttachMode {
                type Error = ::planus::errors::UnknownEnumTagKind;
                #[inline]
                fn try_from(
                    value: i8,
                ) -> ::core::result::Result<Self, ::planus::errors::UnknownEnumTagKind>
                {
                    #[allow(clippy::match_single_binding)]
                    match value {
                        0 => ::core::result::Result::Ok(AttachMode::FromCursor),
                        1 => ::core::result::Result::Ok(AttachMode::LiveOnly),
                        2 => ::core::result::Result::Ok(AttachMode::FromEarliest),

                        _ => ::core::result::Result::Err(::planus::errors::UnknownEnumTagKind {
                            tag: value as i128,
                        }),
                    }
                }
            }

            impl ::core::convert::From<AttachMode> for i8 {
                #[inline]
                fn from(value: AttachMode) -> Self {
                    value as i8
                }
            }

            /// # Safety
            /// The Planus compiler correctly calculates `ALIGNMENT` and `SIZE`.
            unsafe impl ::planus::Primitive for AttachMode {
                const ALIGNMENT: usize = 1;
                const SIZE: usize = 1;
            }

            impl ::planus::WriteAsPrimitive<AttachMode> for AttachMode {
                #[inline]
                fn write<const N: usize>(
                    &self,
                    cursor: ::planus::Cursor<'_, N>,
                    buffer_position: u32,
                ) {
                    (*self as i8).write(cursor, buffer_position);
                }
            }

            impl ::planus::WriteAs<AttachMode> for AttachMode {
                type Prepared = Self;

                #[inline]
                fn prepare(&self, _builder: &mut ::planus::Builder) -> AttachMode {
                    *self
                }
            }

            impl ::planus::WriteAsDefault<AttachMode, AttachMode> for AttachMode {
                type Prepared = Self;

                #[inline]
                fn prepare(
                    &self,
                    _builder: &mut ::planus::Builder,
                    default: &AttachMode,
                ) -> ::core::option::Option<AttachMode> {
                    if self == default {
                        ::core::option::Option::None
                    } else {
                        ::core::option::Option::Some(*self)
                    }
                }
            }

            impl ::planus::WriteAsOptional<AttachMode> for AttachMode {
                type Prepared = Self;

                #[inline]
                fn prepare(
                    &self,
                    _builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<AttachMode> {
                    ::core::option::Option::Some(*self)
                }
            }

            impl<'buf> ::planus::TableRead<'buf> for AttachMode {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'buf>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    let n: i8 = ::planus::TableRead::from_buffer(buffer, offset)?;
                    ::core::result::Result::Ok(::core::convert::TryInto::try_into(n)?)
                }
            }

            impl<'buf> ::planus::VectorReadInner<'buf> for AttachMode {
                type Error = ::planus::errors::UnknownEnumTag;
                const STRIDE: usize = 1;
                #[inline]
                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'buf>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::UnknownEnumTag>
                {
                    let value = unsafe { *buffer.buffer.get_unchecked(offset) as i8 };
                    let value: ::core::result::Result<Self, _> =
                        ::core::convert::TryInto::try_into(value);
                    value.map_err(|error_kind| {
                        error_kind.with_error_location(
                            "AttachMode",
                            "VectorRead::from_buffer",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<AttachMode> for AttachMode {
                const STRIDE: usize = 1;

                type Value = Self;

                #[inline]
                fn prepare(&self, _builder: &mut ::planus::Builder) -> Self {
                    *self
                }

                #[inline]
                unsafe fn write_values(
                    values: &[Self],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 1];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - i as u32,
                        );
                    }
                }
            }

            /// The enum `DetachReason` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Enum `DetachReason` in the file `schema/asmux.fbs:18`
            #[derive(
                Copy,
                Clone,
                Debug,
                PartialEq,
                Eq,
                PartialOrd,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            #[repr(i8)]
            pub enum DetachReason {
                /// The variant `Superseded` in the enum `DetachReason`
                Superseded = 0,

                /// The variant `Killed` in the enum `DetachReason`
                Killed = 1,

                /// The variant `Backpressure` in the enum `DetachReason`
                Backpressure = 2,

                /// The variant `ServerShutdown` in the enum `DetachReason`
                ServerShutdown = 3,

                /// The variant `Purged` in the enum `DetachReason`
                Purged = 4,
            }

            impl DetachReason {
                /// Array containing all valid variants of DetachReason
                pub const ENUM_VALUES: [Self; 5] = [
                    Self::Superseded,
                    Self::Killed,
                    Self::Backpressure,
                    Self::ServerShutdown,
                    Self::Purged,
                ];
            }

            impl ::core::convert::TryFrom<i8> for DetachReason {
                type Error = ::planus::errors::UnknownEnumTagKind;
                #[inline]
                fn try_from(
                    value: i8,
                ) -> ::core::result::Result<Self, ::planus::errors::UnknownEnumTagKind>
                {
                    #[allow(clippy::match_single_binding)]
                    match value {
                        0 => ::core::result::Result::Ok(DetachReason::Superseded),
                        1 => ::core::result::Result::Ok(DetachReason::Killed),
                        2 => ::core::result::Result::Ok(DetachReason::Backpressure),
                        3 => ::core::result::Result::Ok(DetachReason::ServerShutdown),
                        4 => ::core::result::Result::Ok(DetachReason::Purged),

                        _ => ::core::result::Result::Err(::planus::errors::UnknownEnumTagKind {
                            tag: value as i128,
                        }),
                    }
                }
            }

            impl ::core::convert::From<DetachReason> for i8 {
                #[inline]
                fn from(value: DetachReason) -> Self {
                    value as i8
                }
            }

            /// # Safety
            /// The Planus compiler correctly calculates `ALIGNMENT` and `SIZE`.
            unsafe impl ::planus::Primitive for DetachReason {
                const ALIGNMENT: usize = 1;
                const SIZE: usize = 1;
            }

            impl ::planus::WriteAsPrimitive<DetachReason> for DetachReason {
                #[inline]
                fn write<const N: usize>(
                    &self,
                    cursor: ::planus::Cursor<'_, N>,
                    buffer_position: u32,
                ) {
                    (*self as i8).write(cursor, buffer_position);
                }
            }

            impl ::planus::WriteAs<DetachReason> for DetachReason {
                type Prepared = Self;

                #[inline]
                fn prepare(&self, _builder: &mut ::planus::Builder) -> DetachReason {
                    *self
                }
            }

            impl ::planus::WriteAsDefault<DetachReason, DetachReason> for DetachReason {
                type Prepared = Self;

                #[inline]
                fn prepare(
                    &self,
                    _builder: &mut ::planus::Builder,
                    default: &DetachReason,
                ) -> ::core::option::Option<DetachReason> {
                    if self == default {
                        ::core::option::Option::None
                    } else {
                        ::core::option::Option::Some(*self)
                    }
                }
            }

            impl ::planus::WriteAsOptional<DetachReason> for DetachReason {
                type Prepared = Self;

                #[inline]
                fn prepare(
                    &self,
                    _builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<DetachReason> {
                    ::core::option::Option::Some(*self)
                }
            }

            impl<'buf> ::planus::TableRead<'buf> for DetachReason {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'buf>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    let n: i8 = ::planus::TableRead::from_buffer(buffer, offset)?;
                    ::core::result::Result::Ok(::core::convert::TryInto::try_into(n)?)
                }
            }

            impl<'buf> ::planus::VectorReadInner<'buf> for DetachReason {
                type Error = ::planus::errors::UnknownEnumTag;
                const STRIDE: usize = 1;
                #[inline]
                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'buf>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::UnknownEnumTag>
                {
                    let value = unsafe { *buffer.buffer.get_unchecked(offset) as i8 };
                    let value: ::core::result::Result<Self, _> =
                        ::core::convert::TryInto::try_into(value);
                    value.map_err(|error_kind| {
                        error_kind.with_error_location(
                            "DetachReason",
                            "VectorRead::from_buffer",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<DetachReason> for DetachReason {
                const STRIDE: usize = 1;

                type Value = Self;

                #[inline]
                fn prepare(&self, _builder: &mut ::planus::Builder) -> Self {
                    *self
                }

                #[inline]
                unsafe fn write_values(
                    values: &[Self],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 1];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - i as u32,
                        );
                    }
                }
            }

            /// The table `KV` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `KV` in the file `schema/asmux.fbs:26`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct Kv {
                /// The field `key` in the table `KV`
                pub key: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `value` in the table `KV`
                pub value: ::core::option::Option<::planus::alloc::string::String>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for Kv {
                fn default() -> Self {
                    Self {
                        key: ::core::default::Default::default(),
                        value: ::core::default::Default::default(),
                    }
                }
            }

            impl Kv {
                /// Creates a [KvBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> KvBuilder<()> {
                    KvBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_key: impl ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    field_value: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                ) -> ::planus::Offset<Self> {
                    let prepared_key = field_key.prepare(builder);
                    let prepared_value = field_value.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<8> =
                        ::core::default::Default::default();
                    if prepared_key.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(0);
                    }
                    if prepared_value.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_key) = prepared_key {
                                object_writer.write::<_, _, 4>(&prepared_key);
                            }
                            if let ::core::option::Option::Some(prepared_value) = prepared_value {
                                object_writer.write::<_, _, 4>(&prepared_value);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<Kv>> for Kv {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Kv> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<Kv>> for Kv {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<Kv>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<Kv> for Kv {
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Kv> {
                    Kv::create(builder, &self.key, &self.value)
                }
            }

            /// Builder for serializing an instance of the [Kv] type.
            ///
            /// Can be created using the [Kv::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct KvBuilder<State>(State);

            impl KvBuilder<()> {
                /// Setter for the [`key` field](Kv#structfield.key).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn key<T0>(self, value: T0) -> KvBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    KvBuilder((value,))
                }

                /// Sets the [`key` field](Kv#structfield.key) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn key_as_null(self) -> KvBuilder<((),)> {
                    self.key(())
                }
            }

            impl<T0> KvBuilder<(T0,)> {
                /// Setter for the [`value` field](Kv#structfield.value).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn value<T1>(self, value: T1) -> KvBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0,) = self.0;
                    KvBuilder((v0, value))
                }

                /// Sets the [`value` field](Kv#structfield.value) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn value_as_null(self) -> KvBuilder<(T0, ())> {
                    self.value(())
                }
            }

            impl<T0, T1> KvBuilder<(T0, T1)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [Kv].
                #[inline]
                pub fn finish(self, builder: &mut ::planus::Builder) -> ::planus::Offset<Kv>
                where
                    Self: ::planus::WriteAsOffset<Kv>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAs<::planus::Offset<Kv>> for KvBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<Kv>;

                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Kv> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOptional<::planus::Offset<Kv>> for KvBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<Kv>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<Kv>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOffset<Kv> for KvBuilder<(T0, T1)>
            {
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Kv> {
                    let (v0, v1) = &self.0;
                    Kv::create(builder, v0, v1)
                }
            }

            /// Reference to a deserialized [Kv].
            #[derive(Copy, Clone)]
            pub struct KvRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> KvRef<'a> {
                /// Getter for the [`key` field](Kv#structfield.key).
                #[inline]
                pub fn key(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(0, "Kv", "key")
                }

                /// Getter for the [`value` field](Kv#structfield.value).
                #[inline]
                pub fn value(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(1, "Kv", "value")
                }
            }

            impl<'a> ::core::fmt::Debug for KvRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("KvRef");
                    if let ::core::option::Option::Some(field_key) = self.key().transpose() {
                        f.field("key", &field_key);
                    }
                    if let ::core::option::Option::Some(field_value) = self.value().transpose() {
                        f.field("value", &field_value);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<KvRef<'a>> for Kv {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: KvRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        key: value.key()?.map(::core::convert::Into::into),
                        value: value.value()?.map(::core::convert::Into::into),
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for KvRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for KvRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location("[KvRef]", "get", buffer.offset_from_start)
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<Kv>> for Kv {
                type Value = ::planus::Offset<Kv>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<Kv>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for KvRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[KvRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `SessionRecord` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `SessionRecord` in the file `schema/asmux.fbs:28`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct SessionRecord {
                /// The field `id` in the table `SessionRecord`
                pub id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `alive` in the table `SessionRecord`
                pub alive: bool,
                /// The field `pid` in the table `SessionRecord`
                pub pid: i32,
                /// The field `exit_code` in the table `SessionRecord`
                pub exit_code: i32,
                /// The field `exit_signal` in the table `SessionRecord`
                pub exit_signal: i32,
                /// The field `cols` in the table `SessionRecord`
                pub cols: u16,
                /// The field `rows` in the table `SessionRecord`
                pub rows: u16,
                /// The field `head_cursor` in the table `SessionRecord`
                pub head_cursor: u64,
                /// The field `tail_cursor` in the table `SessionRecord`
                pub tail_cursor: u64,
                /// The field `ring_capacity` in the table `SessionRecord`
                pub ring_capacity: u64,
                /// The field `created_at_unix_ms` in the table `SessionRecord`
                pub created_at_unix_ms: i64,
                /// The field `command` in the table `SessionRecord`
                pub command: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `metadata` in the table `SessionRecord`
                pub metadata: ::core::option::Option<::planus::alloc::vec::Vec<self::Kv>>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for SessionRecord {
                fn default() -> Self {
                    Self {
                        id: ::core::default::Default::default(),
                        alive: false,
                        pid: 0,
                        exit_code: 0,
                        exit_signal: 0,
                        cols: 0,
                        rows: 0,
                        head_cursor: 0,
                        tail_cursor: 0,
                        ring_capacity: 0,
                        created_at_unix_ms: 0,
                        command: ::core::default::Default::default(),
                        metadata: ::core::default::Default::default(),
                    }
                }
            }

            impl SessionRecord {
                /// Creates a [SessionRecordBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> SessionRecordBuilder<()> {
                    SessionRecordBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_id: impl ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    field_alive: impl ::planus::WriteAsDefault<bool, bool>,
                    field_pid: impl ::planus::WriteAsDefault<i32, i32>,
                    field_exit_code: impl ::planus::WriteAsDefault<i32, i32>,
                    field_exit_signal: impl ::planus::WriteAsDefault<i32, i32>,
                    field_cols: impl ::planus::WriteAsDefault<u16, u16>,
                    field_rows: impl ::planus::WriteAsDefault<u16, u16>,
                    field_head_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                    field_tail_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                    field_ring_capacity: impl ::planus::WriteAsDefault<u64, u64>,
                    field_created_at_unix_ms: impl ::planus::WriteAsDefault<i64, i64>,
                    field_command: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_metadata: impl ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<self::Kv>]>,
                    >,
                ) -> ::planus::Offset<Self> {
                    let prepared_id = field_id.prepare(builder);
                    let prepared_alive = field_alive.prepare(builder, &false);
                    let prepared_pid = field_pid.prepare(builder, &0);
                    let prepared_exit_code = field_exit_code.prepare(builder, &0);
                    let prepared_exit_signal = field_exit_signal.prepare(builder, &0);
                    let prepared_cols = field_cols.prepare(builder, &0);
                    let prepared_rows = field_rows.prepare(builder, &0);
                    let prepared_head_cursor = field_head_cursor.prepare(builder, &0);
                    let prepared_tail_cursor = field_tail_cursor.prepare(builder, &0);
                    let prepared_ring_capacity = field_ring_capacity.prepare(builder, &0);
                    let prepared_created_at_unix_ms = field_created_at_unix_ms.prepare(builder, &0);
                    let prepared_command = field_command.prepare(builder);
                    let prepared_metadata = field_metadata.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<30> =
                        ::core::default::Default::default();
                    if prepared_head_cursor.is_some() {
                        table_writer.write_entry::<u64>(7);
                    }
                    if prepared_tail_cursor.is_some() {
                        table_writer.write_entry::<u64>(8);
                    }
                    if prepared_ring_capacity.is_some() {
                        table_writer.write_entry::<u64>(9);
                    }
                    if prepared_created_at_unix_ms.is_some() {
                        table_writer.write_entry::<i64>(10);
                    }
                    if prepared_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(0);
                    }
                    if prepared_pid.is_some() {
                        table_writer.write_entry::<i32>(2);
                    }
                    if prepared_exit_code.is_some() {
                        table_writer.write_entry::<i32>(3);
                    }
                    if prepared_exit_signal.is_some() {
                        table_writer.write_entry::<i32>(4);
                    }
                    if prepared_command.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(11);
                    }
                    if prepared_metadata.is_some() {
                        table_writer
                            .write_entry::<::planus::Offset<[::planus::Offset<self::Kv>]>>(12);
                    }
                    if prepared_cols.is_some() {
                        table_writer.write_entry::<u16>(5);
                    }
                    if prepared_rows.is_some() {
                        table_writer.write_entry::<u16>(6);
                    }
                    if prepared_alive.is_some() {
                        table_writer.write_entry::<bool>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_head_cursor) =
                                prepared_head_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_head_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_tail_cursor) =
                                prepared_tail_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_tail_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_ring_capacity) =
                                prepared_ring_capacity
                            {
                                object_writer.write::<_, _, 8>(&prepared_ring_capacity);
                            }
                            if let ::core::option::Option::Some(prepared_created_at_unix_ms) =
                                prepared_created_at_unix_ms
                            {
                                object_writer.write::<_, _, 8>(&prepared_created_at_unix_ms);
                            }
                            if let ::core::option::Option::Some(prepared_id) = prepared_id {
                                object_writer.write::<_, _, 4>(&prepared_id);
                            }
                            if let ::core::option::Option::Some(prepared_pid) = prepared_pid {
                                object_writer.write::<_, _, 4>(&prepared_pid);
                            }
                            if let ::core::option::Option::Some(prepared_exit_code) =
                                prepared_exit_code
                            {
                                object_writer.write::<_, _, 4>(&prepared_exit_code);
                            }
                            if let ::core::option::Option::Some(prepared_exit_signal) =
                                prepared_exit_signal
                            {
                                object_writer.write::<_, _, 4>(&prepared_exit_signal);
                            }
                            if let ::core::option::Option::Some(prepared_command) = prepared_command
                            {
                                object_writer.write::<_, _, 4>(&prepared_command);
                            }
                            if let ::core::option::Option::Some(prepared_metadata) =
                                prepared_metadata
                            {
                                object_writer.write::<_, _, 4>(&prepared_metadata);
                            }
                            if let ::core::option::Option::Some(prepared_cols) = prepared_cols {
                                object_writer.write::<_, _, 2>(&prepared_cols);
                            }
                            if let ::core::option::Option::Some(prepared_rows) = prepared_rows {
                                object_writer.write::<_, _, 2>(&prepared_rows);
                            }
                            if let ::core::option::Option::Some(prepared_alive) = prepared_alive {
                                object_writer.write::<_, _, 1>(&prepared_alive);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<SessionRecord>> for SessionRecord {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionRecord> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<SessionRecord>> for SessionRecord {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionRecord>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<SessionRecord> for SessionRecord {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionRecord> {
                    SessionRecord::create(
                        builder,
                        &self.id,
                        self.alive,
                        self.pid,
                        self.exit_code,
                        self.exit_signal,
                        self.cols,
                        self.rows,
                        self.head_cursor,
                        self.tail_cursor,
                        self.ring_capacity,
                        self.created_at_unix_ms,
                        &self.command,
                        &self.metadata,
                    )
                }
            }

            /// Builder for serializing an instance of the [SessionRecord] type.
            ///
            /// Can be created using the [SessionRecord::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct SessionRecordBuilder<State>(State);

            impl SessionRecordBuilder<()> {
                /// Setter for the [`id` field](SessionRecord#structfield.id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn id<T0>(self, value: T0) -> SessionRecordBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    SessionRecordBuilder((value,))
                }

                /// Sets the [`id` field](SessionRecord#structfield.id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn id_as_null(self) -> SessionRecordBuilder<((),)> {
                    self.id(())
                }
            }

            impl<T0> SessionRecordBuilder<(T0,)> {
                /// Setter for the [`alive` field](SessionRecord#structfield.alive).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn alive<T1>(self, value: T1) -> SessionRecordBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsDefault<bool, bool>,
                {
                    let (v0,) = self.0;
                    SessionRecordBuilder((v0, value))
                }

                /// Sets the [`alive` field](SessionRecord#structfield.alive) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn alive_as_default(
                    self,
                ) -> SessionRecordBuilder<(T0, ::planus::DefaultValue)> {
                    self.alive(::planus::DefaultValue)
                }
            }

            impl<T0, T1> SessionRecordBuilder<(T0, T1)> {
                /// Setter for the [`pid` field](SessionRecord#structfield.pid).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn pid<T2>(self, value: T2) -> SessionRecordBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsDefault<i32, i32>,
                {
                    let (v0, v1) = self.0;
                    SessionRecordBuilder((v0, v1, value))
                }

                /// Sets the [`pid` field](SessionRecord#structfield.pid) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn pid_as_default(
                    self,
                ) -> SessionRecordBuilder<(T0, T1, ::planus::DefaultValue)> {
                    self.pid(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2> SessionRecordBuilder<(T0, T1, T2)> {
                /// Setter for the [`exit_code` field](SessionRecord#structfield.exit_code).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn exit_code<T3>(self, value: T3) -> SessionRecordBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsDefault<i32, i32>,
                {
                    let (v0, v1, v2) = self.0;
                    SessionRecordBuilder((v0, v1, v2, value))
                }

                /// Sets the [`exit_code` field](SessionRecord#structfield.exit_code) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn exit_code_as_default(
                    self,
                ) -> SessionRecordBuilder<(T0, T1, T2, ::planus::DefaultValue)> {
                    self.exit_code(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3> SessionRecordBuilder<(T0, T1, T2, T3)> {
                /// Setter for the [`exit_signal` field](SessionRecord#structfield.exit_signal).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn exit_signal<T4>(
                    self,
                    value: T4,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4)>
                where
                    T4: ::planus::WriteAsDefault<i32, i32>,
                {
                    let (v0, v1, v2, v3) = self.0;
                    SessionRecordBuilder((v0, v1, v2, v3, value))
                }

                /// Sets the [`exit_signal` field](SessionRecord#structfield.exit_signal) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn exit_signal_as_default(
                    self,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, ::planus::DefaultValue)>
                {
                    self.exit_signal(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4> SessionRecordBuilder<(T0, T1, T2, T3, T4)> {
                /// Setter for the [`cols` field](SessionRecord#structfield.cols).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn cols<T5>(self, value: T5) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5)>
                where
                    T5: ::planus::WriteAsDefault<u16, u16>,
                {
                    let (v0, v1, v2, v3, v4) = self.0;
                    SessionRecordBuilder((v0, v1, v2, v3, v4, value))
                }

                /// Sets the [`cols` field](SessionRecord#structfield.cols) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn cols_as_default(
                    self,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, ::planus::DefaultValue)>
                {
                    self.cols(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5)> {
                /// Setter for the [`rows` field](SessionRecord#structfield.rows).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rows<T6>(
                    self,
                    value: T6,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6)>
                where
                    T6: ::planus::WriteAsDefault<u16, u16>,
                {
                    let (v0, v1, v2, v3, v4, v5) = self.0;
                    SessionRecordBuilder((v0, v1, v2, v3, v4, v5, value))
                }

                /// Sets the [`rows` field](SessionRecord#structfield.rows) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rows_as_default(
                    self,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, ::planus::DefaultValue)>
                {
                    self.rows(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6)> {
                /// Setter for the [`head_cursor` field](SessionRecord#structfield.head_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor<T7>(
                    self,
                    value: T7,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7)>
                where
                    T7: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1, v2, v3, v4, v5, v6) = self.0;
                    SessionRecordBuilder((v0, v1, v2, v3, v4, v5, v6, value))
                }

                /// Sets the [`head_cursor` field](SessionRecord#structfield.head_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor_as_default(
                    self,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, ::planus::DefaultValue)>
                {
                    self.head_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6, T7> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7)> {
                /// Setter for the [`tail_cursor` field](SessionRecord#structfield.tail_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn tail_cursor<T8>(
                    self,
                    value: T8,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8)>
                where
                    T8: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1, v2, v3, v4, v5, v6, v7) = self.0;
                    SessionRecordBuilder((v0, v1, v2, v3, v4, v5, v6, v7, value))
                }

                /// Sets the [`tail_cursor` field](SessionRecord#structfield.tail_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn tail_cursor_as_default(
                    self,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, ::planus::DefaultValue)>
                {
                    self.tail_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6, T7, T8>
                SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8)>
            {
                /// Setter for the [`ring_capacity` field](SessionRecord#structfield.ring_capacity).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn ring_capacity<T9>(
                    self,
                    value: T9,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)>
                where
                    T9: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1, v2, v3, v4, v5, v6, v7, v8) = self.0;
                    SessionRecordBuilder((v0, v1, v2, v3, v4, v5, v6, v7, v8, value))
                }

                /// Sets the [`ring_capacity` field](SessionRecord#structfield.ring_capacity) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn ring_capacity_as_default(
                    self,
                ) -> SessionRecordBuilder<(
                    T0,
                    T1,
                    T2,
                    T3,
                    T4,
                    T5,
                    T6,
                    T7,
                    T8,
                    ::planus::DefaultValue,
                )> {
                    self.ring_capacity(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9>
                SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)>
            {
                /// Setter for the [`created_at_unix_ms` field](SessionRecord#structfield.created_at_unix_ms).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn created_at_unix_ms<T10>(
                    self,
                    value: T10,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10)>
                where
                    T10: ::planus::WriteAsDefault<i64, i64>,
                {
                    let (v0, v1, v2, v3, v4, v5, v6, v7, v8, v9) = self.0;
                    SessionRecordBuilder((v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, value))
                }

                /// Sets the [`created_at_unix_ms` field](SessionRecord#structfield.created_at_unix_ms) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn created_at_unix_ms_as_default(
                    self,
                ) -> SessionRecordBuilder<(
                    T0,
                    T1,
                    T2,
                    T3,
                    T4,
                    T5,
                    T6,
                    T7,
                    T8,
                    T9,
                    ::planus::DefaultValue,
                )> {
                    self.created_at_unix_ms(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>
                SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10)>
            {
                /// Setter for the [`command` field](SessionRecord#structfield.command).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn command<T11>(
                    self,
                    value: T11,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11)>
                where
                    T11: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10) = self.0;
                    SessionRecordBuilder((v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, value))
                }

                /// Sets the [`command` field](SessionRecord#structfield.command) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn command_as_null(
                    self,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, ())>
                {
                    self.command(())
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11>
                SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11)>
            {
                /// Setter for the [`metadata` field](SessionRecord#structfield.metadata).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn metadata<T12>(
                    self,
                    value: T12,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12)>
                where
                    T12: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                {
                    let (v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11) = self.0;
                    SessionRecordBuilder((v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, value))
                }

                /// Sets the [`metadata` field](SessionRecord#structfield.metadata) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn metadata_as_null(
                    self,
                ) -> SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, ())>
                {
                    self.metadata(())
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12>
                SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12)>
            {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [SessionRecord].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionRecord>
                where
                    Self: ::planus::WriteAsOffset<SessionRecord>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<bool, bool>,
                    T2: ::planus::WriteAsDefault<i32, i32>,
                    T3: ::planus::WriteAsDefault<i32, i32>,
                    T4: ::planus::WriteAsDefault<i32, i32>,
                    T5: ::planus::WriteAsDefault<u16, u16>,
                    T6: ::planus::WriteAsDefault<u16, u16>,
                    T7: ::planus::WriteAsDefault<u64, u64>,
                    T8: ::planus::WriteAsDefault<u64, u64>,
                    T9: ::planus::WriteAsDefault<u64, u64>,
                    T10: ::planus::WriteAsDefault<i64, i64>,
                    T11: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T12: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                > ::planus::WriteAs<::planus::Offset<SessionRecord>>
                for SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12)>
            {
                type Prepared = ::planus::Offset<SessionRecord>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionRecord> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<bool, bool>,
                    T2: ::planus::WriteAsDefault<i32, i32>,
                    T3: ::planus::WriteAsDefault<i32, i32>,
                    T4: ::planus::WriteAsDefault<i32, i32>,
                    T5: ::planus::WriteAsDefault<u16, u16>,
                    T6: ::planus::WriteAsDefault<u16, u16>,
                    T7: ::planus::WriteAsDefault<u64, u64>,
                    T8: ::planus::WriteAsDefault<u64, u64>,
                    T9: ::planus::WriteAsDefault<u64, u64>,
                    T10: ::planus::WriteAsDefault<i64, i64>,
                    T11: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T12: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                > ::planus::WriteAsOptional<::planus::Offset<SessionRecord>>
                for SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12)>
            {
                type Prepared = ::planus::Offset<SessionRecord>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionRecord>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<bool, bool>,
                    T2: ::planus::WriteAsDefault<i32, i32>,
                    T3: ::planus::WriteAsDefault<i32, i32>,
                    T4: ::planus::WriteAsDefault<i32, i32>,
                    T5: ::planus::WriteAsDefault<u16, u16>,
                    T6: ::planus::WriteAsDefault<u16, u16>,
                    T7: ::planus::WriteAsDefault<u64, u64>,
                    T8: ::planus::WriteAsDefault<u64, u64>,
                    T9: ::planus::WriteAsDefault<u64, u64>,
                    T10: ::planus::WriteAsDefault<i64, i64>,
                    T11: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T12: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                > ::planus::WriteAsOffset<SessionRecord>
                for SessionRecordBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionRecord> {
                    let (v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12) = &self.0;
                    SessionRecord::create(
                        builder, v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10, v11, v12,
                    )
                }
            }

            /// Reference to a deserialized [SessionRecord].
            #[derive(Copy, Clone)]
            pub struct SessionRecordRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> SessionRecordRef<'a> {
                /// Getter for the [`id` field](SessionRecord#structfield.id).
                #[inline]
                pub fn id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(0, "SessionRecord", "id")
                }

                /// Getter for the [`alive` field](SessionRecord#structfield.alive).
                #[inline]
                pub fn alive(&self) -> ::planus::Result<bool> {
                    ::core::result::Result::Ok(
                        self.0.access(1, "SessionRecord", "alive")?.unwrap_or(false),
                    )
                }

                /// Getter for the [`pid` field](SessionRecord#structfield.pid).
                #[inline]
                pub fn pid(&self) -> ::planus::Result<i32> {
                    ::core::result::Result::Ok(
                        self.0.access(2, "SessionRecord", "pid")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`exit_code` field](SessionRecord#structfield.exit_code).
                #[inline]
                pub fn exit_code(&self) -> ::planus::Result<i32> {
                    ::core::result::Result::Ok(
                        self.0.access(3, "SessionRecord", "exit_code")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`exit_signal` field](SessionRecord#structfield.exit_signal).
                #[inline]
                pub fn exit_signal(&self) -> ::planus::Result<i32> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(4, "SessionRecord", "exit_signal")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`cols` field](SessionRecord#structfield.cols).
                #[inline]
                pub fn cols(&self) -> ::planus::Result<u16> {
                    ::core::result::Result::Ok(
                        self.0.access(5, "SessionRecord", "cols")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`rows` field](SessionRecord#structfield.rows).
                #[inline]
                pub fn rows(&self) -> ::planus::Result<u16> {
                    ::core::result::Result::Ok(
                        self.0.access(6, "SessionRecord", "rows")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`head_cursor` field](SessionRecord#structfield.head_cursor).
                #[inline]
                pub fn head_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(7, "SessionRecord", "head_cursor")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`tail_cursor` field](SessionRecord#structfield.tail_cursor).
                #[inline]
                pub fn tail_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(8, "SessionRecord", "tail_cursor")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`ring_capacity` field](SessionRecord#structfield.ring_capacity).
                #[inline]
                pub fn ring_capacity(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(9, "SessionRecord", "ring_capacity")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`created_at_unix_ms` field](SessionRecord#structfield.created_at_unix_ms).
                #[inline]
                pub fn created_at_unix_ms(&self) -> ::planus::Result<i64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(10, "SessionRecord", "created_at_unix_ms")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`command` field](SessionRecord#structfield.command).
                #[inline]
                pub fn command(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(11, "SessionRecord", "command")
                }

                /// Getter for the [`metadata` field](SessionRecord#structfield.metadata).
                #[inline]
                pub fn metadata(
                    &self,
                ) -> ::planus::Result<
                    ::core::option::Option<::planus::Vector<'a, ::planus::Result<self::KvRef<'a>>>>,
                > {
                    self.0.access(12, "SessionRecord", "metadata")
                }
            }

            impl<'a> ::core::fmt::Debug for SessionRecordRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("SessionRecordRef");
                    if let ::core::option::Option::Some(field_id) = self.id().transpose() {
                        f.field("id", &field_id);
                    }
                    f.field("alive", &self.alive());
                    f.field("pid", &self.pid());
                    f.field("exit_code", &self.exit_code());
                    f.field("exit_signal", &self.exit_signal());
                    f.field("cols", &self.cols());
                    f.field("rows", &self.rows());
                    f.field("head_cursor", &self.head_cursor());
                    f.field("tail_cursor", &self.tail_cursor());
                    f.field("ring_capacity", &self.ring_capacity());
                    f.field("created_at_unix_ms", &self.created_at_unix_ms());
                    if let ::core::option::Option::Some(field_command) = self.command().transpose()
                    {
                        f.field("command", &field_command);
                    }
                    if let ::core::option::Option::Some(field_metadata) =
                        self.metadata().transpose()
                    {
                        f.field("metadata", &field_metadata);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<SessionRecordRef<'a>> for SessionRecord {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: SessionRecordRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        id: value.id()?.map(::core::convert::Into::into),
                        alive: ::core::convert::TryInto::try_into(value.alive()?)?,
                        pid: ::core::convert::TryInto::try_into(value.pid()?)?,
                        exit_code: ::core::convert::TryInto::try_into(value.exit_code()?)?,
                        exit_signal: ::core::convert::TryInto::try_into(value.exit_signal()?)?,
                        cols: ::core::convert::TryInto::try_into(value.cols()?)?,
                        rows: ::core::convert::TryInto::try_into(value.rows()?)?,
                        head_cursor: ::core::convert::TryInto::try_into(value.head_cursor()?)?,
                        tail_cursor: ::core::convert::TryInto::try_into(value.tail_cursor()?)?,
                        ring_capacity: ::core::convert::TryInto::try_into(value.ring_capacity()?)?,
                        created_at_unix_ms: ::core::convert::TryInto::try_into(
                            value.created_at_unix_ms()?,
                        )?,
                        command: value.command()?.map(::core::convert::Into::into),
                        metadata: if let ::core::option::Option::Some(metadata) =
                            value.metadata()?
                        {
                            ::core::option::Option::Some(metadata.to_vec_result()?)
                        } else {
                            ::core::option::Option::None
                        },
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for SessionRecordRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for SessionRecordRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[SessionRecordRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<SessionRecord>> for SessionRecord {
                type Value = ::planus::Offset<SessionRecord>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<SessionRecord>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for SessionRecordRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[SessionRecordRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `Error` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `Error` in the file `schema/asmux.fbs:44`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct Error {
                /// The field `rpc_id` in the table `Error`
                pub rpc_id: u64,
                /// The field `code` in the table `Error`
                pub code: u32,
                /// The field `message` in the table `Error`
                pub message: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `session_id` in the table `Error`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `earliest_cursor` in the table `Error`
                pub earliest_cursor: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for Error {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        code: 0,
                        message: ::core::default::Default::default(),
                        session_id: ::core::default::Default::default(),
                        earliest_cursor: 0,
                    }
                }
            }

            impl Error {
                /// Creates a [ErrorBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> ErrorBuilder<()> {
                    ErrorBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_code: impl ::planus::WriteAsDefault<u32, u32>,
                    field_message: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_earliest_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_code = field_code.prepare(builder, &0);
                    let prepared_message = field_message.prepare(builder);
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_earliest_cursor = field_earliest_cursor.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<14> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_earliest_cursor.is_some() {
                        table_writer.write_entry::<u64>(4);
                    }
                    if prepared_code.is_some() {
                        table_writer.write_entry::<u32>(1);
                    }
                    if prepared_message.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(2);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(3);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_earliest_cursor) =
                                prepared_earliest_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_earliest_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_code) = prepared_code {
                                object_writer.write::<_, _, 4>(&prepared_code);
                            }
                            if let ::core::option::Option::Some(prepared_message) = prepared_message
                            {
                                object_writer.write::<_, _, 4>(&prepared_message);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<Error>> for Error {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Error> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<Error>> for Error {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<Error>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<Error> for Error {
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Error> {
                    Error::create(
                        builder,
                        self.rpc_id,
                        self.code,
                        &self.message,
                        &self.session_id,
                        self.earliest_cursor,
                    )
                }
            }

            /// Builder for serializing an instance of the [Error] type.
            ///
            /// Can be created using the [Error::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct ErrorBuilder<State>(State);

            impl ErrorBuilder<()> {
                /// Setter for the [`rpc_id` field](Error#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> ErrorBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    ErrorBuilder((value,))
                }

                /// Sets the [`rpc_id` field](Error#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> ErrorBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> ErrorBuilder<(T0,)> {
                /// Setter for the [`code` field](Error#structfield.code).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn code<T1>(self, value: T1) -> ErrorBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsDefault<u32, u32>,
                {
                    let (v0,) = self.0;
                    ErrorBuilder((v0, value))
                }

                /// Sets the [`code` field](Error#structfield.code) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn code_as_default(self) -> ErrorBuilder<(T0, ::planus::DefaultValue)> {
                    self.code(::planus::DefaultValue)
                }
            }

            impl<T0, T1> ErrorBuilder<(T0, T1)> {
                /// Setter for the [`message` field](Error#structfield.message).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn message<T2>(self, value: T2) -> ErrorBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0, v1) = self.0;
                    ErrorBuilder((v0, v1, value))
                }

                /// Sets the [`message` field](Error#structfield.message) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn message_as_null(self) -> ErrorBuilder<(T0, T1, ())> {
                    self.message(())
                }
            }

            impl<T0, T1, T2> ErrorBuilder<(T0, T1, T2)> {
                /// Setter for the [`session_id` field](Error#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T3>(self, value: T3) -> ErrorBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0, v1, v2) = self.0;
                    ErrorBuilder((v0, v1, v2, value))
                }

                /// Sets the [`session_id` field](Error#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> ErrorBuilder<(T0, T1, T2, ())> {
                    self.session_id(())
                }
            }

            impl<T0, T1, T2, T3> ErrorBuilder<(T0, T1, T2, T3)> {
                /// Setter for the [`earliest_cursor` field](Error#structfield.earliest_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn earliest_cursor<T4>(self, value: T4) -> ErrorBuilder<(T0, T1, T2, T3, T4)>
                where
                    T4: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1, v2, v3) = self.0;
                    ErrorBuilder((v0, v1, v2, v3, value))
                }

                /// Sets the [`earliest_cursor` field](Error#structfield.earliest_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn earliest_cursor_as_default(
                    self,
                ) -> ErrorBuilder<(T0, T1, T2, T3, ::planus::DefaultValue)> {
                    self.earliest_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4> ErrorBuilder<(T0, T1, T2, T3, T4)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [Error].
                #[inline]
                pub fn finish(self, builder: &mut ::planus::Builder) -> ::planus::Offset<Error>
                where
                    Self: ::planus::WriteAsOffset<Error>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<u32, u32>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T3: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T4: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAs<::planus::Offset<Error>>
                for ErrorBuilder<(T0, T1, T2, T3, T4)>
            {
                type Prepared = ::planus::Offset<Error>;

                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Error> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<u32, u32>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T3: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T4: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOptional<::planus::Offset<Error>>
                for ErrorBuilder<(T0, T1, T2, T3, T4)>
            {
                type Prepared = ::planus::Offset<Error>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<Error>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<u32, u32>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T3: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T4: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOffset<Error> for ErrorBuilder<(T0, T1, T2, T3, T4)>
            {
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Error> {
                    let (v0, v1, v2, v3, v4) = &self.0;
                    Error::create(builder, v0, v1, v2, v3, v4)
                }
            }

            /// Reference to a deserialized [Error].
            #[derive(Copy, Clone)]
            pub struct ErrorRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> ErrorRef<'a> {
                /// Getter for the [`rpc_id` field](Error#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(self.0.access(0, "Error", "rpc_id")?.unwrap_or(0))
                }

                /// Getter for the [`code` field](Error#structfield.code).
                #[inline]
                pub fn code(&self) -> ::planus::Result<u32> {
                    ::core::result::Result::Ok(self.0.access(1, "Error", "code")?.unwrap_or(0))
                }

                /// Getter for the [`message` field](Error#structfield.message).
                #[inline]
                pub fn message(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(2, "Error", "message")
                }

                /// Getter for the [`session_id` field](Error#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(3, "Error", "session_id")
                }

                /// Getter for the [`earliest_cursor` field](Error#structfield.earliest_cursor).
                #[inline]
                pub fn earliest_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(4, "Error", "earliest_cursor")?.unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for ErrorRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("ErrorRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.field("code", &self.code());
                    if let ::core::option::Option::Some(field_message) = self.message().transpose()
                    {
                        f.field("message", &field_message);
                    }
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.field("earliest_cursor", &self.earliest_cursor());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<ErrorRef<'a>> for Error {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: ErrorRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        code: ::core::convert::TryInto::try_into(value.code()?)?,
                        message: value.message()?.map(::core::convert::Into::into),
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        earliest_cursor: ::core::convert::TryInto::try_into(
                            value.earliest_cursor()?,
                        )?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for ErrorRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for ErrorRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[ErrorRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<Error>> for Error {
                type Value = ::planus::Offset<Error>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<Error>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for ErrorRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[ErrorRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `HelloRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `HelloRequest` in the file `schema/asmux.fbs:52`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct HelloRequest {
                /// The field `rpc_id` in the table `HelloRequest`
                pub rpc_id: u64,
                /// The field `client_pid` in the table `HelloRequest`
                pub client_pid: i32,
                /// The field `client_name` in the table `HelloRequest`
                pub client_name: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `protocol_min` in the table `HelloRequest`
                pub protocol_min: u16,
                /// The field `protocol_max` in the table `HelloRequest`
                pub protocol_max: u16,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for HelloRequest {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        client_pid: 0,
                        client_name: ::core::default::Default::default(),
                        protocol_min: 0,
                        protocol_max: 0,
                    }
                }
            }

            impl HelloRequest {
                /// Creates a [HelloRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> HelloRequestBuilder<()> {
                    HelloRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_client_pid: impl ::planus::WriteAsDefault<i32, i32>,
                    field_client_name: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_protocol_min: impl ::planus::WriteAsDefault<u16, u16>,
                    field_protocol_max: impl ::planus::WriteAsDefault<u16, u16>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_client_pid = field_client_pid.prepare(builder, &0);
                    let prepared_client_name = field_client_name.prepare(builder);
                    let prepared_protocol_min = field_protocol_min.prepare(builder, &0);
                    let prepared_protocol_max = field_protocol_max.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<14> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_client_pid.is_some() {
                        table_writer.write_entry::<i32>(1);
                    }
                    if prepared_client_name.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(2);
                    }
                    if prepared_protocol_min.is_some() {
                        table_writer.write_entry::<u16>(3);
                    }
                    if prepared_protocol_max.is_some() {
                        table_writer.write_entry::<u16>(4);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_client_pid) =
                                prepared_client_pid
                            {
                                object_writer.write::<_, _, 4>(&prepared_client_pid);
                            }
                            if let ::core::option::Option::Some(prepared_client_name) =
                                prepared_client_name
                            {
                                object_writer.write::<_, _, 4>(&prepared_client_name);
                            }
                            if let ::core::option::Option::Some(prepared_protocol_min) =
                                prepared_protocol_min
                            {
                                object_writer.write::<_, _, 2>(&prepared_protocol_min);
                            }
                            if let ::core::option::Option::Some(prepared_protocol_max) =
                                prepared_protocol_max
                            {
                                object_writer.write::<_, _, 2>(&prepared_protocol_max);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<HelloRequest>> for HelloRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<HelloRequest>> for HelloRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<HelloRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<HelloRequest> for HelloRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloRequest> {
                    HelloRequest::create(
                        builder,
                        self.rpc_id,
                        self.client_pid,
                        &self.client_name,
                        self.protocol_min,
                        self.protocol_max,
                    )
                }
            }

            /// Builder for serializing an instance of the [HelloRequest] type.
            ///
            /// Can be created using the [HelloRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct HelloRequestBuilder<State>(State);

            impl HelloRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](HelloRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> HelloRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    HelloRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](HelloRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> HelloRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> HelloRequestBuilder<(T0,)> {
                /// Setter for the [`client_pid` field](HelloRequest#structfield.client_pid).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn client_pid<T1>(self, value: T1) -> HelloRequestBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsDefault<i32, i32>,
                {
                    let (v0,) = self.0;
                    HelloRequestBuilder((v0, value))
                }

                /// Sets the [`client_pid` field](HelloRequest#structfield.client_pid) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn client_pid_as_default(
                    self,
                ) -> HelloRequestBuilder<(T0, ::planus::DefaultValue)> {
                    self.client_pid(::planus::DefaultValue)
                }
            }

            impl<T0, T1> HelloRequestBuilder<(T0, T1)> {
                /// Setter for the [`client_name` field](HelloRequest#structfield.client_name).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn client_name<T2>(self, value: T2) -> HelloRequestBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0, v1) = self.0;
                    HelloRequestBuilder((v0, v1, value))
                }

                /// Sets the [`client_name` field](HelloRequest#structfield.client_name) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn client_name_as_null(self) -> HelloRequestBuilder<(T0, T1, ())> {
                    self.client_name(())
                }
            }

            impl<T0, T1, T2> HelloRequestBuilder<(T0, T1, T2)> {
                /// Setter for the [`protocol_min` field](HelloRequest#structfield.protocol_min).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn protocol_min<T3>(self, value: T3) -> HelloRequestBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsDefault<u16, u16>,
                {
                    let (v0, v1, v2) = self.0;
                    HelloRequestBuilder((v0, v1, v2, value))
                }

                /// Sets the [`protocol_min` field](HelloRequest#structfield.protocol_min) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn protocol_min_as_default(
                    self,
                ) -> HelloRequestBuilder<(T0, T1, T2, ::planus::DefaultValue)> {
                    self.protocol_min(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3> HelloRequestBuilder<(T0, T1, T2, T3)> {
                /// Setter for the [`protocol_max` field](HelloRequest#structfield.protocol_max).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn protocol_max<T4>(
                    self,
                    value: T4,
                ) -> HelloRequestBuilder<(T0, T1, T2, T3, T4)>
                where
                    T4: ::planus::WriteAsDefault<u16, u16>,
                {
                    let (v0, v1, v2, v3) = self.0;
                    HelloRequestBuilder((v0, v1, v2, v3, value))
                }

                /// Sets the [`protocol_max` field](HelloRequest#structfield.protocol_max) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn protocol_max_as_default(
                    self,
                ) -> HelloRequestBuilder<(T0, T1, T2, T3, ::planus::DefaultValue)> {
                    self.protocol_max(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4> HelloRequestBuilder<(T0, T1, T2, T3, T4)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [HelloRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloRequest>
                where
                    Self: ::planus::WriteAsOffset<HelloRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<i32, i32>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T3: ::planus::WriteAsDefault<u16, u16>,
                    T4: ::planus::WriteAsDefault<u16, u16>,
                > ::planus::WriteAs<::planus::Offset<HelloRequest>>
                for HelloRequestBuilder<(T0, T1, T2, T3, T4)>
            {
                type Prepared = ::planus::Offset<HelloRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<i32, i32>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T3: ::planus::WriteAsDefault<u16, u16>,
                    T4: ::planus::WriteAsDefault<u16, u16>,
                > ::planus::WriteAsOptional<::planus::Offset<HelloRequest>>
                for HelloRequestBuilder<(T0, T1, T2, T3, T4)>
            {
                type Prepared = ::planus::Offset<HelloRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<HelloRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<i32, i32>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T3: ::planus::WriteAsDefault<u16, u16>,
                    T4: ::planus::WriteAsDefault<u16, u16>,
                > ::planus::WriteAsOffset<HelloRequest>
                for HelloRequestBuilder<(T0, T1, T2, T3, T4)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloRequest> {
                    let (v0, v1, v2, v3, v4) = &self.0;
                    HelloRequest::create(builder, v0, v1, v2, v3, v4)
                }
            }

            /// Reference to a deserialized [HelloRequest].
            #[derive(Copy, Clone)]
            pub struct HelloRequestRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> HelloRequestRef<'a> {
                /// Getter for the [`rpc_id` field](HelloRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "HelloRequest", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`client_pid` field](HelloRequest#structfield.client_pid).
                #[inline]
                pub fn client_pid(&self) -> ::planus::Result<i32> {
                    ::core::result::Result::Ok(
                        self.0.access(1, "HelloRequest", "client_pid")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`client_name` field](HelloRequest#structfield.client_name).
                #[inline]
                pub fn client_name(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(2, "HelloRequest", "client_name")
                }

                /// Getter for the [`protocol_min` field](HelloRequest#structfield.protocol_min).
                #[inline]
                pub fn protocol_min(&self) -> ::planus::Result<u16> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(3, "HelloRequest", "protocol_min")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`protocol_max` field](HelloRequest#structfield.protocol_max).
                #[inline]
                pub fn protocol_max(&self) -> ::planus::Result<u16> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(4, "HelloRequest", "protocol_max")?
                            .unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for HelloRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("HelloRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.field("client_pid", &self.client_pid());
                    if let ::core::option::Option::Some(field_client_name) =
                        self.client_name().transpose()
                    {
                        f.field("client_name", &field_client_name);
                    }
                    f.field("protocol_min", &self.protocol_min());
                    f.field("protocol_max", &self.protocol_max());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<HelloRequestRef<'a>> for HelloRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: HelloRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        client_pid: ::core::convert::TryInto::try_into(value.client_pid()?)?,
                        client_name: value.client_name()?.map(::core::convert::Into::into),
                        protocol_min: ::core::convert::TryInto::try_into(value.protocol_min()?)?,
                        protocol_max: ::core::convert::TryInto::try_into(value.protocol_max()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for HelloRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for HelloRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[HelloRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<HelloRequest>> for HelloRequest {
                type Value = ::planus::Offset<HelloRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<HelloRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for HelloRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[HelloRequestRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `HelloResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `HelloResponse` in the file `schema/asmux.fbs:59`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct HelloResponse {
                /// The field `rpc_id` in the table `HelloResponse`
                pub rpc_id: u64,
                /// The field `server_pid` in the table `HelloResponse`
                pub server_pid: i32,
                /// The field `binary_sha256` in the table `HelloResponse`
                pub binary_sha256: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `protocol` in the table `HelloResponse`
                pub protocol: u16,
                /// The field `session_count` in the table `HelloResponse`
                pub session_count: u32,
                /// The field `started_at_unix_ms` in the table `HelloResponse`
                pub started_at_unix_ms: i64,
                /// The field `instance_id` in the table `HelloResponse`
                pub instance_id: ::core::option::Option<::planus::alloc::string::String>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for HelloResponse {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        server_pid: 0,
                        binary_sha256: ::core::default::Default::default(),
                        protocol: 0,
                        session_count: 0,
                        started_at_unix_ms: 0,
                        instance_id: ::core::default::Default::default(),
                    }
                }
            }

            impl HelloResponse {
                /// Creates a [HelloResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> HelloResponseBuilder<()> {
                    HelloResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_server_pid: impl ::planus::WriteAsDefault<i32, i32>,
                    field_binary_sha256: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_protocol: impl ::planus::WriteAsDefault<u16, u16>,
                    field_session_count: impl ::planus::WriteAsDefault<u32, u32>,
                    field_started_at_unix_ms: impl ::planus::WriteAsDefault<i64, i64>,
                    field_instance_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_server_pid = field_server_pid.prepare(builder, &0);
                    let prepared_binary_sha256 = field_binary_sha256.prepare(builder);
                    let prepared_protocol = field_protocol.prepare(builder, &0);
                    let prepared_session_count = field_session_count.prepare(builder, &0);
                    let prepared_started_at_unix_ms = field_started_at_unix_ms.prepare(builder, &0);
                    let prepared_instance_id = field_instance_id.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<18> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_started_at_unix_ms.is_some() {
                        table_writer.write_entry::<i64>(5);
                    }
                    if prepared_server_pid.is_some() {
                        table_writer.write_entry::<i32>(1);
                    }
                    if prepared_binary_sha256.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(2);
                    }
                    if prepared_session_count.is_some() {
                        table_writer.write_entry::<u32>(4);
                    }
                    if prepared_instance_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(6);
                    }
                    if prepared_protocol.is_some() {
                        table_writer.write_entry::<u16>(3);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_started_at_unix_ms) =
                                prepared_started_at_unix_ms
                            {
                                object_writer.write::<_, _, 8>(&prepared_started_at_unix_ms);
                            }
                            if let ::core::option::Option::Some(prepared_server_pid) =
                                prepared_server_pid
                            {
                                object_writer.write::<_, _, 4>(&prepared_server_pid);
                            }
                            if let ::core::option::Option::Some(prepared_binary_sha256) =
                                prepared_binary_sha256
                            {
                                object_writer.write::<_, _, 4>(&prepared_binary_sha256);
                            }
                            if let ::core::option::Option::Some(prepared_session_count) =
                                prepared_session_count
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_count);
                            }
                            if let ::core::option::Option::Some(prepared_instance_id) =
                                prepared_instance_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_instance_id);
                            }
                            if let ::core::option::Option::Some(prepared_protocol) =
                                prepared_protocol
                            {
                                object_writer.write::<_, _, 2>(&prepared_protocol);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<HelloResponse>> for HelloResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<HelloResponse>> for HelloResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<HelloResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<HelloResponse> for HelloResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloResponse> {
                    HelloResponse::create(
                        builder,
                        self.rpc_id,
                        self.server_pid,
                        &self.binary_sha256,
                        self.protocol,
                        self.session_count,
                        self.started_at_unix_ms,
                        &self.instance_id,
                    )
                }
            }

            /// Builder for serializing an instance of the [HelloResponse] type.
            ///
            /// Can be created using the [HelloResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct HelloResponseBuilder<State>(State);

            impl HelloResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](HelloResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> HelloResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    HelloResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](HelloResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> HelloResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> HelloResponseBuilder<(T0,)> {
                /// Setter for the [`server_pid` field](HelloResponse#structfield.server_pid).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn server_pid<T1>(self, value: T1) -> HelloResponseBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsDefault<i32, i32>,
                {
                    let (v0,) = self.0;
                    HelloResponseBuilder((v0, value))
                }

                /// Sets the [`server_pid` field](HelloResponse#structfield.server_pid) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn server_pid_as_default(
                    self,
                ) -> HelloResponseBuilder<(T0, ::planus::DefaultValue)> {
                    self.server_pid(::planus::DefaultValue)
                }
            }

            impl<T0, T1> HelloResponseBuilder<(T0, T1)> {
                /// Setter for the [`binary_sha256` field](HelloResponse#structfield.binary_sha256).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn binary_sha256<T2>(self, value: T2) -> HelloResponseBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0, v1) = self.0;
                    HelloResponseBuilder((v0, v1, value))
                }

                /// Sets the [`binary_sha256` field](HelloResponse#structfield.binary_sha256) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn binary_sha256_as_null(self) -> HelloResponseBuilder<(T0, T1, ())> {
                    self.binary_sha256(())
                }
            }

            impl<T0, T1, T2> HelloResponseBuilder<(T0, T1, T2)> {
                /// Setter for the [`protocol` field](HelloResponse#structfield.protocol).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn protocol<T3>(self, value: T3) -> HelloResponseBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsDefault<u16, u16>,
                {
                    let (v0, v1, v2) = self.0;
                    HelloResponseBuilder((v0, v1, v2, value))
                }

                /// Sets the [`protocol` field](HelloResponse#structfield.protocol) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn protocol_as_default(
                    self,
                ) -> HelloResponseBuilder<(T0, T1, T2, ::planus::DefaultValue)> {
                    self.protocol(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3> HelloResponseBuilder<(T0, T1, T2, T3)> {
                /// Setter for the [`session_count` field](HelloResponse#structfield.session_count).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_count<T4>(
                    self,
                    value: T4,
                ) -> HelloResponseBuilder<(T0, T1, T2, T3, T4)>
                where
                    T4: ::planus::WriteAsDefault<u32, u32>,
                {
                    let (v0, v1, v2, v3) = self.0;
                    HelloResponseBuilder((v0, v1, v2, v3, value))
                }

                /// Sets the [`session_count` field](HelloResponse#structfield.session_count) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_count_as_default(
                    self,
                ) -> HelloResponseBuilder<(T0, T1, T2, T3, ::planus::DefaultValue)>
                {
                    self.session_count(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4> HelloResponseBuilder<(T0, T1, T2, T3, T4)> {
                /// Setter for the [`started_at_unix_ms` field](HelloResponse#structfield.started_at_unix_ms).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn started_at_unix_ms<T5>(
                    self,
                    value: T5,
                ) -> HelloResponseBuilder<(T0, T1, T2, T3, T4, T5)>
                where
                    T5: ::planus::WriteAsDefault<i64, i64>,
                {
                    let (v0, v1, v2, v3, v4) = self.0;
                    HelloResponseBuilder((v0, v1, v2, v3, v4, value))
                }

                /// Sets the [`started_at_unix_ms` field](HelloResponse#structfield.started_at_unix_ms) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn started_at_unix_ms_as_default(
                    self,
                ) -> HelloResponseBuilder<(T0, T1, T2, T3, T4, ::planus::DefaultValue)>
                {
                    self.started_at_unix_ms(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5> HelloResponseBuilder<(T0, T1, T2, T3, T4, T5)> {
                /// Setter for the [`instance_id` field](HelloResponse#structfield.instance_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn instance_id<T6>(
                    self,
                    value: T6,
                ) -> HelloResponseBuilder<(T0, T1, T2, T3, T4, T5, T6)>
                where
                    T6: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0, v1, v2, v3, v4, v5) = self.0;
                    HelloResponseBuilder((v0, v1, v2, v3, v4, v5, value))
                }

                /// Sets the [`instance_id` field](HelloResponse#structfield.instance_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn instance_id_as_null(
                    self,
                ) -> HelloResponseBuilder<(T0, T1, T2, T3, T4, T5, ())> {
                    self.instance_id(())
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6> HelloResponseBuilder<(T0, T1, T2, T3, T4, T5, T6)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [HelloResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloResponse>
                where
                    Self: ::planus::WriteAsOffset<HelloResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<i32, i32>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T3: ::planus::WriteAsDefault<u16, u16>,
                    T4: ::planus::WriteAsDefault<u32, u32>,
                    T5: ::planus::WriteAsDefault<i64, i64>,
                    T6: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAs<::planus::Offset<HelloResponse>>
                for HelloResponseBuilder<(T0, T1, T2, T3, T4, T5, T6)>
            {
                type Prepared = ::planus::Offset<HelloResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<i32, i32>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T3: ::planus::WriteAsDefault<u16, u16>,
                    T4: ::planus::WriteAsDefault<u32, u32>,
                    T5: ::planus::WriteAsDefault<i64, i64>,
                    T6: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOptional<::planus::Offset<HelloResponse>>
                for HelloResponseBuilder<(T0, T1, T2, T3, T4, T5, T6)>
            {
                type Prepared = ::planus::Offset<HelloResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<HelloResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<i32, i32>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T3: ::planus::WriteAsDefault<u16, u16>,
                    T4: ::planus::WriteAsDefault<u32, u32>,
                    T5: ::planus::WriteAsDefault<i64, i64>,
                    T6: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOffset<HelloResponse>
                for HelloResponseBuilder<(T0, T1, T2, T3, T4, T5, T6)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<HelloResponse> {
                    let (v0, v1, v2, v3, v4, v5, v6) = &self.0;
                    HelloResponse::create(builder, v0, v1, v2, v3, v4, v5, v6)
                }
            }

            /// Reference to a deserialized [HelloResponse].
            #[derive(Copy, Clone)]
            pub struct HelloResponseRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> HelloResponseRef<'a> {
                /// Getter for the [`rpc_id` field](HelloResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "HelloResponse", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`server_pid` field](HelloResponse#structfield.server_pid).
                #[inline]
                pub fn server_pid(&self) -> ::planus::Result<i32> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(1, "HelloResponse", "server_pid")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`binary_sha256` field](HelloResponse#structfield.binary_sha256).
                #[inline]
                pub fn binary_sha256(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(2, "HelloResponse", "binary_sha256")
                }

                /// Getter for the [`protocol` field](HelloResponse#structfield.protocol).
                #[inline]
                pub fn protocol(&self) -> ::planus::Result<u16> {
                    ::core::result::Result::Ok(
                        self.0.access(3, "HelloResponse", "protocol")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`session_count` field](HelloResponse#structfield.session_count).
                #[inline]
                pub fn session_count(&self) -> ::planus::Result<u32> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(4, "HelloResponse", "session_count")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`started_at_unix_ms` field](HelloResponse#structfield.started_at_unix_ms).
                #[inline]
                pub fn started_at_unix_ms(&self) -> ::planus::Result<i64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(5, "HelloResponse", "started_at_unix_ms")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`instance_id` field](HelloResponse#structfield.instance_id).
                #[inline]
                pub fn instance_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(6, "HelloResponse", "instance_id")
                }
            }

            impl<'a> ::core::fmt::Debug for HelloResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("HelloResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.field("server_pid", &self.server_pid());
                    if let ::core::option::Option::Some(field_binary_sha256) =
                        self.binary_sha256().transpose()
                    {
                        f.field("binary_sha256", &field_binary_sha256);
                    }
                    f.field("protocol", &self.protocol());
                    f.field("session_count", &self.session_count());
                    f.field("started_at_unix_ms", &self.started_at_unix_ms());
                    if let ::core::option::Option::Some(field_instance_id) =
                        self.instance_id().transpose()
                    {
                        f.field("instance_id", &field_instance_id);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<HelloResponseRef<'a>> for HelloResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: HelloResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        server_pid: ::core::convert::TryInto::try_into(value.server_pid()?)?,
                        binary_sha256: value.binary_sha256()?.map(::core::convert::Into::into),
                        protocol: ::core::convert::TryInto::try_into(value.protocol()?)?,
                        session_count: ::core::convert::TryInto::try_into(value.session_count()?)?,
                        started_at_unix_ms: ::core::convert::TryInto::try_into(
                            value.started_at_unix_ms()?,
                        )?,
                        instance_id: value.instance_id()?.map(::core::convert::Into::into),
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for HelloResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for HelloResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[HelloResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<HelloResponse>> for HelloResponse {
                type Value = ::planus::Offset<HelloResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<HelloResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for HelloResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[HelloResponseRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `CreateRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `CreateRequest` in the file `schema/asmux.fbs:69`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct CreateRequest {
                /// The field `rpc_id` in the table `CreateRequest`
                pub rpc_id: u64,
                /// The field `command` in the table `CreateRequest`
                pub command: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `args` in the table `CreateRequest`
                pub args: ::core::option::Option<
                    ::planus::alloc::vec::Vec<::planus::alloc::string::String>,
                >,
                /// The field `cwd` in the table `CreateRequest`
                pub cwd: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `env` in the table `CreateRequest`
                pub env: ::core::option::Option<::planus::alloc::vec::Vec<self::Kv>>,
                /// The field `cols` in the table `CreateRequest`
                pub cols: u16,
                /// The field `rows` in the table `CreateRequest`
                pub rows: u16,
                /// The field `metadata` in the table `CreateRequest`
                pub metadata: ::core::option::Option<::planus::alloc::vec::Vec<self::Kv>>,
                /// The field `ring_capacity` in the table `CreateRequest`
                pub ring_capacity: u64,
                /// The field `session_id` in the table `CreateRequest`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for CreateRequest {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        command: ::core::default::Default::default(),
                        args: ::core::default::Default::default(),
                        cwd: ::core::default::Default::default(),
                        env: ::core::default::Default::default(),
                        cols: 0,
                        rows: 0,
                        metadata: ::core::default::Default::default(),
                        ring_capacity: 0,
                        session_id: ::core::default::Default::default(),
                    }
                }
            }

            impl CreateRequest {
                /// Creates a [CreateRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> CreateRequestBuilder<()> {
                    CreateRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_command: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_args: impl ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<str>]>,
                    >,
                    field_cwd: impl ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    field_env: impl ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<self::Kv>]>,
                    >,
                    field_cols: impl ::planus::WriteAsDefault<u16, u16>,
                    field_rows: impl ::planus::WriteAsDefault<u16, u16>,
                    field_metadata: impl ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<self::Kv>]>,
                    >,
                    field_ring_capacity: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_command = field_command.prepare(builder);
                    let prepared_args = field_args.prepare(builder);
                    let prepared_cwd = field_cwd.prepare(builder);
                    let prepared_env = field_env.prepare(builder);
                    let prepared_cols = field_cols.prepare(builder, &0);
                    let prepared_rows = field_rows.prepare(builder, &0);
                    let prepared_metadata = field_metadata.prepare(builder);
                    let prepared_ring_capacity = field_ring_capacity.prepare(builder, &0);
                    let prepared_session_id = field_session_id.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<24> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_ring_capacity.is_some() {
                        table_writer.write_entry::<u64>(8);
                    }
                    if prepared_command.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(1);
                    }
                    if prepared_args.is_some() {
                        table_writer.write_entry::<::planus::Offset<[::planus::Offset<str>]>>(2);
                    }
                    if prepared_cwd.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(3);
                    }
                    if prepared_env.is_some() {
                        table_writer
                            .write_entry::<::planus::Offset<[::planus::Offset<self::Kv>]>>(4);
                    }
                    if prepared_metadata.is_some() {
                        table_writer
                            .write_entry::<::planus::Offset<[::planus::Offset<self::Kv>]>>(7);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(9);
                    }
                    if prepared_cols.is_some() {
                        table_writer.write_entry::<u16>(5);
                    }
                    if prepared_rows.is_some() {
                        table_writer.write_entry::<u16>(6);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_ring_capacity) =
                                prepared_ring_capacity
                            {
                                object_writer.write::<_, _, 8>(&prepared_ring_capacity);
                            }
                            if let ::core::option::Option::Some(prepared_command) = prepared_command
                            {
                                object_writer.write::<_, _, 4>(&prepared_command);
                            }
                            if let ::core::option::Option::Some(prepared_args) = prepared_args {
                                object_writer.write::<_, _, 4>(&prepared_args);
                            }
                            if let ::core::option::Option::Some(prepared_cwd) = prepared_cwd {
                                object_writer.write::<_, _, 4>(&prepared_cwd);
                            }
                            if let ::core::option::Option::Some(prepared_env) = prepared_env {
                                object_writer.write::<_, _, 4>(&prepared_env);
                            }
                            if let ::core::option::Option::Some(prepared_metadata) =
                                prepared_metadata
                            {
                                object_writer.write::<_, _, 4>(&prepared_metadata);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                            if let ::core::option::Option::Some(prepared_cols) = prepared_cols {
                                object_writer.write::<_, _, 2>(&prepared_cols);
                            }
                            if let ::core::option::Option::Some(prepared_rows) = prepared_rows {
                                object_writer.write::<_, _, 2>(&prepared_rows);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<CreateRequest>> for CreateRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<CreateRequest>> for CreateRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<CreateRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<CreateRequest> for CreateRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateRequest> {
                    CreateRequest::create(
                        builder,
                        self.rpc_id,
                        &self.command,
                        &self.args,
                        &self.cwd,
                        &self.env,
                        self.cols,
                        self.rows,
                        &self.metadata,
                        self.ring_capacity,
                        &self.session_id,
                    )
                }
            }

            /// Builder for serializing an instance of the [CreateRequest] type.
            ///
            /// Can be created using the [CreateRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct CreateRequestBuilder<State>(State);

            impl CreateRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](CreateRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> CreateRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    CreateRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](CreateRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> CreateRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> CreateRequestBuilder<(T0,)> {
                /// Setter for the [`command` field](CreateRequest#structfield.command).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn command<T1>(self, value: T1) -> CreateRequestBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0,) = self.0;
                    CreateRequestBuilder((v0, value))
                }

                /// Sets the [`command` field](CreateRequest#structfield.command) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn command_as_null(self) -> CreateRequestBuilder<(T0, ())> {
                    self.command(())
                }
            }

            impl<T0, T1> CreateRequestBuilder<(T0, T1)> {
                /// Setter for the [`args` field](CreateRequest#structfield.args).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn args<T2>(self, value: T2) -> CreateRequestBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<str>]>>,
                {
                    let (v0, v1) = self.0;
                    CreateRequestBuilder((v0, v1, value))
                }

                /// Sets the [`args` field](CreateRequest#structfield.args) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn args_as_null(self) -> CreateRequestBuilder<(T0, T1, ())> {
                    self.args(())
                }
            }

            impl<T0, T1, T2> CreateRequestBuilder<(T0, T1, T2)> {
                /// Setter for the [`cwd` field](CreateRequest#structfield.cwd).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn cwd<T3>(self, value: T3) -> CreateRequestBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0, v1, v2) = self.0;
                    CreateRequestBuilder((v0, v1, v2, value))
                }

                /// Sets the [`cwd` field](CreateRequest#structfield.cwd) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn cwd_as_null(self) -> CreateRequestBuilder<(T0, T1, T2, ())> {
                    self.cwd(())
                }
            }

            impl<T0, T1, T2, T3> CreateRequestBuilder<(T0, T1, T2, T3)> {
                /// Setter for the [`env` field](CreateRequest#structfield.env).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn env<T4>(self, value: T4) -> CreateRequestBuilder<(T0, T1, T2, T3, T4)>
                where
                    T4: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                {
                    let (v0, v1, v2, v3) = self.0;
                    CreateRequestBuilder((v0, v1, v2, v3, value))
                }

                /// Sets the [`env` field](CreateRequest#structfield.env) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn env_as_null(self) -> CreateRequestBuilder<(T0, T1, T2, T3, ())> {
                    self.env(())
                }
            }

            impl<T0, T1, T2, T3, T4> CreateRequestBuilder<(T0, T1, T2, T3, T4)> {
                /// Setter for the [`cols` field](CreateRequest#structfield.cols).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn cols<T5>(self, value: T5) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5)>
                where
                    T5: ::planus::WriteAsDefault<u16, u16>,
                {
                    let (v0, v1, v2, v3, v4) = self.0;
                    CreateRequestBuilder((v0, v1, v2, v3, v4, value))
                }

                /// Sets the [`cols` field](CreateRequest#structfield.cols) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn cols_as_default(
                    self,
                ) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, ::planus::DefaultValue)>
                {
                    self.cols(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5)> {
                /// Setter for the [`rows` field](CreateRequest#structfield.rows).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rows<T6>(
                    self,
                    value: T6,
                ) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6)>
                where
                    T6: ::planus::WriteAsDefault<u16, u16>,
                {
                    let (v0, v1, v2, v3, v4, v5) = self.0;
                    CreateRequestBuilder((v0, v1, v2, v3, v4, v5, value))
                }

                /// Sets the [`rows` field](CreateRequest#structfield.rows) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rows_as_default(
                    self,
                ) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, ::planus::DefaultValue)>
                {
                    self.rows(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6)> {
                /// Setter for the [`metadata` field](CreateRequest#structfield.metadata).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn metadata<T7>(
                    self,
                    value: T7,
                ) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7)>
                where
                    T7: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                {
                    let (v0, v1, v2, v3, v4, v5, v6) = self.0;
                    CreateRequestBuilder((v0, v1, v2, v3, v4, v5, v6, value))
                }

                /// Sets the [`metadata` field](CreateRequest#structfield.metadata) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn metadata_as_null(
                    self,
                ) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, ())> {
                    self.metadata(())
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6, T7> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7)> {
                /// Setter for the [`ring_capacity` field](CreateRequest#structfield.ring_capacity).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn ring_capacity<T8>(
                    self,
                    value: T8,
                ) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8)>
                where
                    T8: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1, v2, v3, v4, v5, v6, v7) = self.0;
                    CreateRequestBuilder((v0, v1, v2, v3, v4, v5, v6, v7, value))
                }

                /// Sets the [`ring_capacity` field](CreateRequest#structfield.ring_capacity) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn ring_capacity_as_default(
                    self,
                ) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, ::planus::DefaultValue)>
                {
                    self.ring_capacity(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6, T7, T8>
                CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8)>
            {
                /// Setter for the [`session_id` field](CreateRequest#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T9>(
                    self,
                    value: T9,
                ) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)>
                where
                    T9: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0, v1, v2, v3, v4, v5, v6, v7, v8) = self.0;
                    CreateRequestBuilder((v0, v1, v2, v3, v4, v5, v6, v7, v8, value))
                }

                /// Sets the [`session_id` field](CreateRequest#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(
                    self,
                ) -> CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, ())>
                {
                    self.session_id(())
                }
            }

            impl<T0, T1, T2, T3, T4, T5, T6, T7, T8, T9>
                CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)>
            {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [CreateRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateRequest>
                where
                    Self: ::planus::WriteAsOffset<CreateRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<str>]>>,
                    T3: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T4: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                    T5: ::planus::WriteAsDefault<u16, u16>,
                    T6: ::planus::WriteAsDefault<u16, u16>,
                    T7: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                    T8: ::planus::WriteAsDefault<u64, u64>,
                    T9: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAs<::planus::Offset<CreateRequest>>
                for CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)>
            {
                type Prepared = ::planus::Offset<CreateRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<str>]>>,
                    T3: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T4: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                    T5: ::planus::WriteAsDefault<u16, u16>,
                    T6: ::planus::WriteAsDefault<u16, u16>,
                    T7: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                    T8: ::planus::WriteAsDefault<u64, u64>,
                    T9: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOptional<::planus::Offset<CreateRequest>>
                for CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)>
            {
                type Prepared = ::planus::Offset<CreateRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<CreateRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<str>]>>,
                    T3: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T4: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                    T5: ::planus::WriteAsDefault<u16, u16>,
                    T6: ::planus::WriteAsDefault<u16, u16>,
                    T7: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                    T8: ::planus::WriteAsDefault<u64, u64>,
                    T9: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOffset<CreateRequest>
                for CreateRequestBuilder<(T0, T1, T2, T3, T4, T5, T6, T7, T8, T9)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateRequest> {
                    let (v0, v1, v2, v3, v4, v5, v6, v7, v8, v9) = &self.0;
                    CreateRequest::create(builder, v0, v1, v2, v3, v4, v5, v6, v7, v8, v9)
                }
            }

            /// Reference to a deserialized [CreateRequest].
            #[derive(Copy, Clone)]
            pub struct CreateRequestRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> CreateRequestRef<'a> {
                /// Getter for the [`rpc_id` field](CreateRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "CreateRequest", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`command` field](CreateRequest#structfield.command).
                #[inline]
                pub fn command(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(1, "CreateRequest", "command")
                }

                /// Getter for the [`args` field](CreateRequest#structfield.args).
                #[inline]
                pub fn args(
                    &self,
                ) -> ::planus::Result<
                    ::core::option::Option<
                        ::planus::Vector<'a, ::planus::Result<&'a ::core::primitive::str>>,
                    >,
                > {
                    self.0.access(2, "CreateRequest", "args")
                }

                /// Getter for the [`cwd` field](CreateRequest#structfield.cwd).
                #[inline]
                pub fn cwd(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(3, "CreateRequest", "cwd")
                }

                /// Getter for the [`env` field](CreateRequest#structfield.env).
                #[inline]
                pub fn env(
                    &self,
                ) -> ::planus::Result<
                    ::core::option::Option<::planus::Vector<'a, ::planus::Result<self::KvRef<'a>>>>,
                > {
                    self.0.access(4, "CreateRequest", "env")
                }

                /// Getter for the [`cols` field](CreateRequest#structfield.cols).
                #[inline]
                pub fn cols(&self) -> ::planus::Result<u16> {
                    ::core::result::Result::Ok(
                        self.0.access(5, "CreateRequest", "cols")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`rows` field](CreateRequest#structfield.rows).
                #[inline]
                pub fn rows(&self) -> ::planus::Result<u16> {
                    ::core::result::Result::Ok(
                        self.0.access(6, "CreateRequest", "rows")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`metadata` field](CreateRequest#structfield.metadata).
                #[inline]
                pub fn metadata(
                    &self,
                ) -> ::planus::Result<
                    ::core::option::Option<::planus::Vector<'a, ::planus::Result<self::KvRef<'a>>>>,
                > {
                    self.0.access(7, "CreateRequest", "metadata")
                }

                /// Getter for the [`ring_capacity` field](CreateRequest#structfield.ring_capacity).
                #[inline]
                pub fn ring_capacity(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(8, "CreateRequest", "ring_capacity")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`session_id` field](CreateRequest#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(9, "CreateRequest", "session_id")
                }
            }

            impl<'a> ::core::fmt::Debug for CreateRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("CreateRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_command) = self.command().transpose()
                    {
                        f.field("command", &field_command);
                    }
                    if let ::core::option::Option::Some(field_args) = self.args().transpose() {
                        f.field("args", &field_args);
                    }
                    if let ::core::option::Option::Some(field_cwd) = self.cwd().transpose() {
                        f.field("cwd", &field_cwd);
                    }
                    if let ::core::option::Option::Some(field_env) = self.env().transpose() {
                        f.field("env", &field_env);
                    }
                    f.field("cols", &self.cols());
                    f.field("rows", &self.rows());
                    if let ::core::option::Option::Some(field_metadata) =
                        self.metadata().transpose()
                    {
                        f.field("metadata", &field_metadata);
                    }
                    f.field("ring_capacity", &self.ring_capacity());
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<CreateRequestRef<'a>> for CreateRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: CreateRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        command: value.command()?.map(::core::convert::Into::into),
                        args: if let ::core::option::Option::Some(args) = value.args()? {
                            ::core::option::Option::Some(args.to_vec_result()?)
                        } else {
                            ::core::option::Option::None
                        },
                        cwd: value.cwd()?.map(::core::convert::Into::into),
                        env: if let ::core::option::Option::Some(env) = value.env()? {
                            ::core::option::Option::Some(env.to_vec_result()?)
                        } else {
                            ::core::option::Option::None
                        },
                        cols: ::core::convert::TryInto::try_into(value.cols()?)?,
                        rows: ::core::convert::TryInto::try_into(value.rows()?)?,
                        metadata: if let ::core::option::Option::Some(metadata) =
                            value.metadata()?
                        {
                            ::core::option::Option::Some(metadata.to_vec_result()?)
                        } else {
                            ::core::option::Option::None
                        },
                        ring_capacity: ::core::convert::TryInto::try_into(value.ring_capacity()?)?,
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for CreateRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for CreateRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[CreateRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<CreateRequest>> for CreateRequest {
                type Value = ::planus::Offset<CreateRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<CreateRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for CreateRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[CreateRequestRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `CreateResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `CreateResponse` in the file `schema/asmux.fbs:81`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct CreateResponse {
                /// The field `rpc_id` in the table `CreateResponse`
                pub rpc_id: u64,
                /// The field `session` in the table `CreateResponse`
                pub session:
                    ::core::option::Option<::planus::alloc::boxed::Box<self::SessionRecord>>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for CreateResponse {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        session: ::core::default::Default::default(),
                    }
                }
            }

            impl CreateResponse {
                /// Creates a [CreateResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> CreateResponseBuilder<()> {
                    CreateResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session: impl ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_session = field_session.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<8> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_session.is_some() {
                        table_writer.write_entry::<::planus::Offset<self::SessionRecord>>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_session) = prepared_session
                            {
                                object_writer.write::<_, _, 4>(&prepared_session);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<CreateResponse>> for CreateResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<CreateResponse>> for CreateResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<CreateResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<CreateResponse> for CreateResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateResponse> {
                    CreateResponse::create(builder, self.rpc_id, &self.session)
                }
            }

            /// Builder for serializing an instance of the [CreateResponse] type.
            ///
            /// Can be created using the [CreateResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct CreateResponseBuilder<State>(State);

            impl CreateResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](CreateResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> CreateResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    CreateResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](CreateResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> CreateResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> CreateResponseBuilder<(T0,)> {
                /// Setter for the [`session` field](CreateResponse#structfield.session).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session<T1>(self, value: T1) -> CreateResponseBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                {
                    let (v0,) = self.0;
                    CreateResponseBuilder((v0, value))
                }

                /// Sets the [`session` field](CreateResponse#structfield.session) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_as_null(self) -> CreateResponseBuilder<(T0, ())> {
                    self.session(())
                }
            }

            impl<T0, T1> CreateResponseBuilder<(T0, T1)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [CreateResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateResponse>
                where
                    Self: ::planus::WriteAsOffset<CreateResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                > ::planus::WriteAs<::planus::Offset<CreateResponse>>
                for CreateResponseBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<CreateResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                > ::planus::WriteAsOptional<::planus::Offset<CreateResponse>>
                for CreateResponseBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<CreateResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<CreateResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                > ::planus::WriteAsOffset<CreateResponse> for CreateResponseBuilder<(T0, T1)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<CreateResponse> {
                    let (v0, v1) = &self.0;
                    CreateResponse::create(builder, v0, v1)
                }
            }

            /// Reference to a deserialized [CreateResponse].
            #[derive(Copy, Clone)]
            pub struct CreateResponseRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> CreateResponseRef<'a> {
                /// Getter for the [`rpc_id` field](CreateResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "CreateResponse", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`session` field](CreateResponse#structfield.session).
                #[inline]
                pub fn session(
                    &self,
                ) -> ::planus::Result<::core::option::Option<self::SessionRecordRef<'a>>>
                {
                    self.0.access(1, "CreateResponse", "session")
                }
            }

            impl<'a> ::core::fmt::Debug for CreateResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("CreateResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_session) = self.session().transpose()
                    {
                        f.field("session", &field_session);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<CreateResponseRef<'a>> for CreateResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: CreateResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        session: if let ::core::option::Option::Some(session) = value.session()? {
                            ::core::option::Option::Some(::planus::alloc::boxed::Box::new(
                                ::core::convert::TryInto::try_into(session)?,
                            ))
                        } else {
                            ::core::option::Option::None
                        },
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for CreateResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for CreateResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[CreateResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<CreateResponse>> for CreateResponse {
                type Value = ::planus::Offset<CreateResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<CreateResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for CreateResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[CreateResponseRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `KillRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `KillRequest` in the file `schema/asmux.fbs:83`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct KillRequest {
                /// The field `rpc_id` in the table `KillRequest`
                pub rpc_id: u64,
                /// The field `session_id` in the table `KillRequest`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `signal` in the table `KillRequest`
                pub signal: i32,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for KillRequest {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        session_id: ::core::default::Default::default(),
                        signal: 0,
                    }
                }
            }

            impl KillRequest {
                /// Creates a [KillRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> KillRequestBuilder<()> {
                    KillRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_signal: impl ::planus::WriteAsDefault<i32, i32>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_signal = field_signal.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<10> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(1);
                    }
                    if prepared_signal.is_some() {
                        table_writer.write_entry::<i32>(2);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                            if let ::core::option::Option::Some(prepared_signal) = prepared_signal {
                                object_writer.write::<_, _, 4>(&prepared_signal);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<KillRequest>> for KillRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<KillRequest>> for KillRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<KillRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<KillRequest> for KillRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillRequest> {
                    KillRequest::create(builder, self.rpc_id, &self.session_id, self.signal)
                }
            }

            /// Builder for serializing an instance of the [KillRequest] type.
            ///
            /// Can be created using the [KillRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct KillRequestBuilder<State>(State);

            impl KillRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](KillRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> KillRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    KillRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](KillRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> KillRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> KillRequestBuilder<(T0,)> {
                /// Setter for the [`session_id` field](KillRequest#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T1>(self, value: T1) -> KillRequestBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0,) = self.0;
                    KillRequestBuilder((v0, value))
                }

                /// Sets the [`session_id` field](KillRequest#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> KillRequestBuilder<(T0, ())> {
                    self.session_id(())
                }
            }

            impl<T0, T1> KillRequestBuilder<(T0, T1)> {
                /// Setter for the [`signal` field](KillRequest#structfield.signal).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn signal<T2>(self, value: T2) -> KillRequestBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsDefault<i32, i32>,
                {
                    let (v0, v1) = self.0;
                    KillRequestBuilder((v0, v1, value))
                }

                /// Sets the [`signal` field](KillRequest#structfield.signal) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn signal_as_default(
                    self,
                ) -> KillRequestBuilder<(T0, T1, ::planus::DefaultValue)> {
                    self.signal(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2> KillRequestBuilder<(T0, T1, T2)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [KillRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillRequest>
                where
                    Self: ::planus::WriteAsOffset<KillRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<i32, i32>,
                > ::planus::WriteAs<::planus::Offset<KillRequest>>
                for KillRequestBuilder<(T0, T1, T2)>
            {
                type Prepared = ::planus::Offset<KillRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<i32, i32>,
                > ::planus::WriteAsOptional<::planus::Offset<KillRequest>>
                for KillRequestBuilder<(T0, T1, T2)>
            {
                type Prepared = ::planus::Offset<KillRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<KillRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<i32, i32>,
                > ::planus::WriteAsOffset<KillRequest> for KillRequestBuilder<(T0, T1, T2)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillRequest> {
                    let (v0, v1, v2) = &self.0;
                    KillRequest::create(builder, v0, v1, v2)
                }
            }

            /// Reference to a deserialized [KillRequest].
            #[derive(Copy, Clone)]
            pub struct KillRequestRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> KillRequestRef<'a> {
                /// Getter for the [`rpc_id` field](KillRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "KillRequest", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`session_id` field](KillRequest#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(1, "KillRequest", "session_id")
                }

                /// Getter for the [`signal` field](KillRequest#structfield.signal).
                #[inline]
                pub fn signal(&self) -> ::planus::Result<i32> {
                    ::core::result::Result::Ok(
                        self.0.access(2, "KillRequest", "signal")?.unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for KillRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("KillRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.field("signal", &self.signal());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<KillRequestRef<'a>> for KillRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: KillRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        signal: ::core::convert::TryInto::try_into(value.signal()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for KillRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for KillRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[KillRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<KillRequest>> for KillRequest {
                type Value = ::planus::Offset<KillRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<KillRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for KillRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[KillRequestRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `KillResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `KillResponse` in the file `schema/asmux.fbs:88`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct KillResponse {
                /// The field `rpc_id` in the table `KillResponse`
                pub rpc_id: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for KillResponse {
                fn default() -> Self {
                    Self { rpc_id: 0 }
                }
            }

            impl KillResponse {
                /// Creates a [KillResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> KillResponseBuilder<()> {
                    KillResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<6> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<KillResponse>> for KillResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<KillResponse>> for KillResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<KillResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<KillResponse> for KillResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillResponse> {
                    KillResponse::create(builder, self.rpc_id)
                }
            }

            /// Builder for serializing an instance of the [KillResponse] type.
            ///
            /// Can be created using the [KillResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct KillResponseBuilder<State>(State);

            impl KillResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](KillResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> KillResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    KillResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](KillResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> KillResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> KillResponseBuilder<(T0,)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [KillResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillResponse>
                where
                    Self: ::planus::WriteAsOffset<KillResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAs<::planus::Offset<KillResponse>> for KillResponseBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<KillResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAsOptional<::planus::Offset<KillResponse>>
                for KillResponseBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<KillResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<KillResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>> ::planus::WriteAsOffset<KillResponse>
                for KillResponseBuilder<(T0,)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<KillResponse> {
                    let (v0,) = &self.0;
                    KillResponse::create(builder, v0)
                }
            }

            /// Reference to a deserialized [KillResponse].
            #[derive(Copy, Clone)]
            pub struct KillResponseRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> KillResponseRef<'a> {
                /// Getter for the [`rpc_id` field](KillResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "KillResponse", "rpc_id")?.unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for KillResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("KillResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<KillResponseRef<'a>> for KillResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: KillResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for KillResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for KillResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[KillResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<KillResponse>> for KillResponse {
                type Value = ::planus::Offset<KillResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<KillResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for KillResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[KillResponseRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `PurgeRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `PurgeRequest` in the file `schema/asmux.fbs:90`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct PurgeRequest {
                /// The field `rpc_id` in the table `PurgeRequest`
                pub rpc_id: u64,
                /// The field `session_id` in the table `PurgeRequest`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for PurgeRequest {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        session_id: ::core::default::Default::default(),
                    }
                }
            }

            impl PurgeRequest {
                /// Creates a [PurgeRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> PurgeRequestBuilder<()> {
                    PurgeRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_session_id = field_session_id.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<8> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<PurgeRequest>> for PurgeRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<PurgeRequest>> for PurgeRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<PurgeRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<PurgeRequest> for PurgeRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeRequest> {
                    PurgeRequest::create(builder, self.rpc_id, &self.session_id)
                }
            }

            /// Builder for serializing an instance of the [PurgeRequest] type.
            ///
            /// Can be created using the [PurgeRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct PurgeRequestBuilder<State>(State);

            impl PurgeRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](PurgeRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> PurgeRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    PurgeRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](PurgeRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> PurgeRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> PurgeRequestBuilder<(T0,)> {
                /// Setter for the [`session_id` field](PurgeRequest#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T1>(self, value: T1) -> PurgeRequestBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0,) = self.0;
                    PurgeRequestBuilder((v0, value))
                }

                /// Sets the [`session_id` field](PurgeRequest#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> PurgeRequestBuilder<(T0, ())> {
                    self.session_id(())
                }
            }

            impl<T0, T1> PurgeRequestBuilder<(T0, T1)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [PurgeRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeRequest>
                where
                    Self: ::planus::WriteAsOffset<PurgeRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAs<::planus::Offset<PurgeRequest>>
                for PurgeRequestBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<PurgeRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOptional<::planus::Offset<PurgeRequest>>
                for PurgeRequestBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<PurgeRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<PurgeRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOffset<PurgeRequest> for PurgeRequestBuilder<(T0, T1)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeRequest> {
                    let (v0, v1) = &self.0;
                    PurgeRequest::create(builder, v0, v1)
                }
            }

            /// Reference to a deserialized [PurgeRequest].
            #[derive(Copy, Clone)]
            pub struct PurgeRequestRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> PurgeRequestRef<'a> {
                /// Getter for the [`rpc_id` field](PurgeRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "PurgeRequest", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`session_id` field](PurgeRequest#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(1, "PurgeRequest", "session_id")
                }
            }

            impl<'a> ::core::fmt::Debug for PurgeRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("PurgeRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<PurgeRequestRef<'a>> for PurgeRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: PurgeRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for PurgeRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for PurgeRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[PurgeRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<PurgeRequest>> for PurgeRequest {
                type Value = ::planus::Offset<PurgeRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<PurgeRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for PurgeRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[PurgeRequestRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `PurgeResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `PurgeResponse` in the file `schema/asmux.fbs:91`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct PurgeResponse {
                /// The field `rpc_id` in the table `PurgeResponse`
                pub rpc_id: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for PurgeResponse {
                fn default() -> Self {
                    Self { rpc_id: 0 }
                }
            }

            impl PurgeResponse {
                /// Creates a [PurgeResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> PurgeResponseBuilder<()> {
                    PurgeResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<6> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<PurgeResponse>> for PurgeResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<PurgeResponse>> for PurgeResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<PurgeResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<PurgeResponse> for PurgeResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeResponse> {
                    PurgeResponse::create(builder, self.rpc_id)
                }
            }

            /// Builder for serializing an instance of the [PurgeResponse] type.
            ///
            /// Can be created using the [PurgeResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct PurgeResponseBuilder<State>(State);

            impl PurgeResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](PurgeResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> PurgeResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    PurgeResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](PurgeResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> PurgeResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> PurgeResponseBuilder<(T0,)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [PurgeResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeResponse>
                where
                    Self: ::planus::WriteAsOffset<PurgeResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAs<::planus::Offset<PurgeResponse>> for PurgeResponseBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<PurgeResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAsOptional<::planus::Offset<PurgeResponse>>
                for PurgeResponseBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<PurgeResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<PurgeResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>> ::planus::WriteAsOffset<PurgeResponse>
                for PurgeResponseBuilder<(T0,)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<PurgeResponse> {
                    let (v0,) = &self.0;
                    PurgeResponse::create(builder, v0)
                }
            }

            /// Reference to a deserialized [PurgeResponse].
            #[derive(Copy, Clone)]
            pub struct PurgeResponseRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> PurgeResponseRef<'a> {
                /// Getter for the [`rpc_id` field](PurgeResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "PurgeResponse", "rpc_id")?.unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for PurgeResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("PurgeResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<PurgeResponseRef<'a>> for PurgeResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: PurgeResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for PurgeResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for PurgeResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[PurgeResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<PurgeResponse>> for PurgeResponse {
                type Value = ::planus::Offset<PurgeResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<PurgeResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for PurgeResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[PurgeResponseRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `ListRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `ListRequest` in the file `schema/asmux.fbs:93`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct ListRequest {
                /// The field `rpc_id` in the table `ListRequest`
                pub rpc_id: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for ListRequest {
                fn default() -> Self {
                    Self { rpc_id: 0 }
                }
            }

            impl ListRequest {
                /// Creates a [ListRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> ListRequestBuilder<()> {
                    ListRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<6> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<ListRequest>> for ListRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<ListRequest>> for ListRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ListRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<ListRequest> for ListRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListRequest> {
                    ListRequest::create(builder, self.rpc_id)
                }
            }

            /// Builder for serializing an instance of the [ListRequest] type.
            ///
            /// Can be created using the [ListRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct ListRequestBuilder<State>(State);

            impl ListRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](ListRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> ListRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    ListRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](ListRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> ListRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> ListRequestBuilder<(T0,)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [ListRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListRequest>
                where
                    Self: ::planus::WriteAsOffset<ListRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAs<::planus::Offset<ListRequest>> for ListRequestBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<ListRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAsOptional<::planus::Offset<ListRequest>>
                for ListRequestBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<ListRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ListRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>> ::planus::WriteAsOffset<ListRequest>
                for ListRequestBuilder<(T0,)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListRequest> {
                    let (v0,) = &self.0;
                    ListRequest::create(builder, v0)
                }
            }

            /// Reference to a deserialized [ListRequest].
            #[derive(Copy, Clone)]
            pub struct ListRequestRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> ListRequestRef<'a> {
                /// Getter for the [`rpc_id` field](ListRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "ListRequest", "rpc_id")?.unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for ListRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("ListRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<ListRequestRef<'a>> for ListRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: ListRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for ListRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for ListRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[ListRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<ListRequest>> for ListRequest {
                type Value = ::planus::Offset<ListRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<ListRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for ListRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[ListRequestRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `ListResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `ListResponse` in the file `schema/asmux.fbs:94`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct ListResponse {
                /// The field `rpc_id` in the table `ListResponse`
                pub rpc_id: u64,
                /// The field `sessions` in the table `ListResponse`
                pub sessions:
                    ::core::option::Option<::planus::alloc::vec::Vec<self::SessionRecord>>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for ListResponse {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        sessions: ::core::default::Default::default(),
                    }
                }
            }

            impl ListResponse {
                /// Creates a [ListResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> ListResponseBuilder<()> {
                    ListResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_sessions: impl ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<self::SessionRecord>]>,
                    >,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_sessions = field_sessions.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<8> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_sessions.is_some() {
                        table_writer.write_entry::<::planus::Offset<[::planus::Offset<self::SessionRecord>]>>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_sessions) =
                                prepared_sessions
                            {
                                object_writer.write::<_, _, 4>(&prepared_sessions);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<ListResponse>> for ListResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<ListResponse>> for ListResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ListResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<ListResponse> for ListResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListResponse> {
                    ListResponse::create(builder, self.rpc_id, &self.sessions)
                }
            }

            /// Builder for serializing an instance of the [ListResponse] type.
            ///
            /// Can be created using the [ListResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct ListResponseBuilder<State>(State);

            impl ListResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](ListResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> ListResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    ListResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](ListResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> ListResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> ListResponseBuilder<(T0,)> {
                /// Setter for the [`sessions` field](ListResponse#structfield.sessions).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn sessions<T1>(self, value: T1) -> ListResponseBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<self::SessionRecord>]>,
                    >,
                {
                    let (v0,) = self.0;
                    ListResponseBuilder((v0, value))
                }

                /// Sets the [`sessions` field](ListResponse#structfield.sessions) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn sessions_as_null(self) -> ListResponseBuilder<(T0, ())> {
                    self.sessions(())
                }
            }

            impl<T0, T1> ListResponseBuilder<(T0, T1)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [ListResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListResponse>
                where
                    Self: ::planus::WriteAsOffset<ListResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<self::SessionRecord>]>,
                    >,
                > ::planus::WriteAs<::planus::Offset<ListResponse>>
                for ListResponseBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<ListResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<self::SessionRecord>]>,
                    >,
                > ::planus::WriteAsOptional<::planus::Offset<ListResponse>>
                for ListResponseBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<ListResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ListResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<self::SessionRecord>]>,
                    >,
                > ::planus::WriteAsOffset<ListResponse> for ListResponseBuilder<(T0, T1)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ListResponse> {
                    let (v0, v1) = &self.0;
                    ListResponse::create(builder, v0, v1)
                }
            }

            /// Reference to a deserialized [ListResponse].
            #[derive(Copy, Clone)]
            pub struct ListResponseRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> ListResponseRef<'a> {
                /// Getter for the [`rpc_id` field](ListResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "ListResponse", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`sessions` field](ListResponse#structfield.sessions).
                #[inline]
                pub fn sessions(
                    &self,
                ) -> ::planus::Result<
                    ::core::option::Option<
                        ::planus::Vector<'a, ::planus::Result<self::SessionRecordRef<'a>>>,
                    >,
                > {
                    self.0.access(1, "ListResponse", "sessions")
                }
            }

            impl<'a> ::core::fmt::Debug for ListResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("ListResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_sessions) =
                        self.sessions().transpose()
                    {
                        f.field("sessions", &field_sessions);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<ListResponseRef<'a>> for ListResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: ListResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        sessions: if let ::core::option::Option::Some(sessions) =
                            value.sessions()?
                        {
                            ::core::option::Option::Some(sessions.to_vec_result()?)
                        } else {
                            ::core::option::Option::None
                        },
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for ListResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for ListResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[ListResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<ListResponse>> for ListResponse {
                type Value = ::planus::Offset<ListResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<ListResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for ListResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[ListResponseRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `UpdateMetadataRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `UpdateMetadataRequest` in the file `schema/asmux.fbs:96`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct UpdateMetadataRequest {
                /// The field `rpc_id` in the table `UpdateMetadataRequest`
                pub rpc_id: u64,
                /// The field `session_id` in the table `UpdateMetadataRequest`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `patch` in the table `UpdateMetadataRequest`
                pub patch: ::core::option::Option<::planus::alloc::vec::Vec<self::Kv>>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for UpdateMetadataRequest {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        session_id: ::core::default::Default::default(),
                        patch: ::core::default::Default::default(),
                    }
                }
            }

            impl UpdateMetadataRequest {
                /// Creates a [UpdateMetadataRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> UpdateMetadataRequestBuilder<()> {
                    UpdateMetadataRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_patch: impl ::planus::WriteAsOptional<
                        ::planus::Offset<[::planus::Offset<self::Kv>]>,
                    >,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_patch = field_patch.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<10> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(1);
                    }
                    if prepared_patch.is_some() {
                        table_writer
                            .write_entry::<::planus::Offset<[::planus::Offset<self::Kv>]>>(2);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                            if let ::core::option::Option::Some(prepared_patch) = prepared_patch {
                                object_writer.write::<_, _, 4>(&prepared_patch);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<UpdateMetadataRequest>> for UpdateMetadataRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<UpdateMetadataRequest>> for UpdateMetadataRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<UpdateMetadataRequest>>
                {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<UpdateMetadataRequest> for UpdateMetadataRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataRequest> {
                    UpdateMetadataRequest::create(
                        builder,
                        self.rpc_id,
                        &self.session_id,
                        &self.patch,
                    )
                }
            }

            /// Builder for serializing an instance of the [UpdateMetadataRequest] type.
            ///
            /// Can be created using the [UpdateMetadataRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct UpdateMetadataRequestBuilder<State>(State);

            impl UpdateMetadataRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](UpdateMetadataRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> UpdateMetadataRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    UpdateMetadataRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](UpdateMetadataRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(
                    self,
                ) -> UpdateMetadataRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> UpdateMetadataRequestBuilder<(T0,)> {
                /// Setter for the [`session_id` field](UpdateMetadataRequest#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T1>(self, value: T1) -> UpdateMetadataRequestBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0,) = self.0;
                    UpdateMetadataRequestBuilder((v0, value))
                }

                /// Sets the [`session_id` field](UpdateMetadataRequest#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> UpdateMetadataRequestBuilder<(T0, ())> {
                    self.session_id(())
                }
            }

            impl<T0, T1> UpdateMetadataRequestBuilder<(T0, T1)> {
                /// Setter for the [`patch` field](UpdateMetadataRequest#structfield.patch).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn patch<T2>(self, value: T2) -> UpdateMetadataRequestBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                {
                    let (v0, v1) = self.0;
                    UpdateMetadataRequestBuilder((v0, v1, value))
                }

                /// Sets the [`patch` field](UpdateMetadataRequest#structfield.patch) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn patch_as_null(self) -> UpdateMetadataRequestBuilder<(T0, T1, ())> {
                    self.patch(())
                }
            }

            impl<T0, T1, T2> UpdateMetadataRequestBuilder<(T0, T1, T2)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [UpdateMetadataRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataRequest>
                where
                    Self: ::planus::WriteAsOffset<UpdateMetadataRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                > ::planus::WriteAs<::planus::Offset<UpdateMetadataRequest>>
                for UpdateMetadataRequestBuilder<(T0, T1, T2)>
            {
                type Prepared = ::planus::Offset<UpdateMetadataRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                > ::planus::WriteAsOptional<::planus::Offset<UpdateMetadataRequest>>
                for UpdateMetadataRequestBuilder<(T0, T1, T2)>
            {
                type Prepared = ::planus::Offset<UpdateMetadataRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<UpdateMetadataRequest>>
                {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<[::planus::Offset<self::Kv>]>>,
                > ::planus::WriteAsOffset<UpdateMetadataRequest>
                for UpdateMetadataRequestBuilder<(T0, T1, T2)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataRequest> {
                    let (v0, v1, v2) = &self.0;
                    UpdateMetadataRequest::create(builder, v0, v1, v2)
                }
            }

            /// Reference to a deserialized [UpdateMetadataRequest].
            #[derive(Copy, Clone)]
            pub struct UpdateMetadataRequestRef<'a>(
                #[allow(dead_code)] ::planus::table_reader::Table<'a>,
            );

            impl<'a> UpdateMetadataRequestRef<'a> {
                /// Getter for the [`rpc_id` field](UpdateMetadataRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(0, "UpdateMetadataRequest", "rpc_id")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`session_id` field](UpdateMetadataRequest#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(1, "UpdateMetadataRequest", "session_id")
                }

                /// Getter for the [`patch` field](UpdateMetadataRequest#structfield.patch).
                #[inline]
                pub fn patch(
                    &self,
                ) -> ::planus::Result<
                    ::core::option::Option<::planus::Vector<'a, ::planus::Result<self::KvRef<'a>>>>,
                > {
                    self.0.access(2, "UpdateMetadataRequest", "patch")
                }
            }

            impl<'a> ::core::fmt::Debug for UpdateMetadataRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("UpdateMetadataRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    if let ::core::option::Option::Some(field_patch) = self.patch().transpose() {
                        f.field("patch", &field_patch);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<UpdateMetadataRequestRef<'a>> for UpdateMetadataRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: UpdateMetadataRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        patch: if let ::core::option::Option::Some(patch) = value.patch()? {
                            ::core::option::Option::Some(patch.to_vec_result()?)
                        } else {
                            ::core::option::Option::None
                        },
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for UpdateMetadataRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for UpdateMetadataRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[UpdateMetadataRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<UpdateMetadataRequest>>
                for UpdateMetadataRequest
            {
                type Value = ::planus::Offset<UpdateMetadataRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<UpdateMetadataRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for UpdateMetadataRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[UpdateMetadataRequestRef]",
                            "read_as_root",
                            0,
                        )
                    })
                }
            }

            /// The table `UpdateMetadataResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `UpdateMetadataResponse` in the file `schema/asmux.fbs:101`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct UpdateMetadataResponse {
                /// The field `rpc_id` in the table `UpdateMetadataResponse`
                pub rpc_id: u64,
                /// The field `session` in the table `UpdateMetadataResponse`
                pub session:
                    ::core::option::Option<::planus::alloc::boxed::Box<self::SessionRecord>>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for UpdateMetadataResponse {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        session: ::core::default::Default::default(),
                    }
                }
            }

            impl UpdateMetadataResponse {
                /// Creates a [UpdateMetadataResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> UpdateMetadataResponseBuilder<()> {
                    UpdateMetadataResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session: impl ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_session = field_session.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<8> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_session.is_some() {
                        table_writer.write_entry::<::planus::Offset<self::SessionRecord>>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_session) = prepared_session
                            {
                                object_writer.write::<_, _, 4>(&prepared_session);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<UpdateMetadataResponse>> for UpdateMetadataResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<UpdateMetadataResponse>>
                for UpdateMetadataResponse
            {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<UpdateMetadataResponse>>
                {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<UpdateMetadataResponse> for UpdateMetadataResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataResponse> {
                    UpdateMetadataResponse::create(builder, self.rpc_id, &self.session)
                }
            }

            /// Builder for serializing an instance of the [UpdateMetadataResponse] type.
            ///
            /// Can be created using the [UpdateMetadataResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct UpdateMetadataResponseBuilder<State>(State);

            impl UpdateMetadataResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](UpdateMetadataResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> UpdateMetadataResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    UpdateMetadataResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](UpdateMetadataResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(
                    self,
                ) -> UpdateMetadataResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> UpdateMetadataResponseBuilder<(T0,)> {
                /// Setter for the [`session` field](UpdateMetadataResponse#structfield.session).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session<T1>(self, value: T1) -> UpdateMetadataResponseBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                {
                    let (v0,) = self.0;
                    UpdateMetadataResponseBuilder((v0, value))
                }

                /// Sets the [`session` field](UpdateMetadataResponse#structfield.session) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_as_null(self) -> UpdateMetadataResponseBuilder<(T0, ())> {
                    self.session(())
                }
            }

            impl<T0, T1> UpdateMetadataResponseBuilder<(T0, T1)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [UpdateMetadataResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataResponse>
                where
                    Self: ::planus::WriteAsOffset<UpdateMetadataResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                > ::planus::WriteAs<::planus::Offset<UpdateMetadataResponse>>
                for UpdateMetadataResponseBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<UpdateMetadataResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                > ::planus::WriteAsOptional<::planus::Offset<UpdateMetadataResponse>>
                for UpdateMetadataResponseBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<UpdateMetadataResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<UpdateMetadataResponse>>
                {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<self::SessionRecord>>,
                > ::planus::WriteAsOffset<UpdateMetadataResponse>
                for UpdateMetadataResponseBuilder<(T0, T1)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<UpdateMetadataResponse> {
                    let (v0, v1) = &self.0;
                    UpdateMetadataResponse::create(builder, v0, v1)
                }
            }

            /// Reference to a deserialized [UpdateMetadataResponse].
            #[derive(Copy, Clone)]
            pub struct UpdateMetadataResponseRef<'a>(
                #[allow(dead_code)] ::planus::table_reader::Table<'a>,
            );

            impl<'a> UpdateMetadataResponseRef<'a> {
                /// Getter for the [`rpc_id` field](UpdateMetadataResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(0, "UpdateMetadataResponse", "rpc_id")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`session` field](UpdateMetadataResponse#structfield.session).
                #[inline]
                pub fn session(
                    &self,
                ) -> ::planus::Result<::core::option::Option<self::SessionRecordRef<'a>>>
                {
                    self.0.access(1, "UpdateMetadataResponse", "session")
                }
            }

            impl<'a> ::core::fmt::Debug for UpdateMetadataResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("UpdateMetadataResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_session) = self.session().transpose()
                    {
                        f.field("session", &field_session);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<UpdateMetadataResponseRef<'a>> for UpdateMetadataResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: UpdateMetadataResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        session: if let ::core::option::Option::Some(session) = value.session()? {
                            ::core::option::Option::Some(::planus::alloc::boxed::Box::new(
                                ::core::convert::TryInto::try_into(session)?,
                            ))
                        } else {
                            ::core::option::Option::None
                        },
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for UpdateMetadataResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for UpdateMetadataResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[UpdateMetadataResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<UpdateMetadataResponse>>
                for UpdateMetadataResponse
            {
                type Value = ::planus::Offset<UpdateMetadataResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<UpdateMetadataResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for UpdateMetadataResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[UpdateMetadataResponseRef]",
                            "read_as_root",
                            0,
                        )
                    })
                }
            }

            /// The table `ResizeRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `ResizeRequest` in the file `schema/asmux.fbs:103`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct ResizeRequest {
                /// The field `rpc_id` in the table `ResizeRequest`
                pub rpc_id: u64,
                /// The field `session_id` in the table `ResizeRequest`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `cols` in the table `ResizeRequest`
                pub cols: u16,
                /// The field `rows` in the table `ResizeRequest`
                pub rows: u16,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for ResizeRequest {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        session_id: ::core::default::Default::default(),
                        cols: 0,
                        rows: 0,
                    }
                }
            }

            impl ResizeRequest {
                /// Creates a [ResizeRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> ResizeRequestBuilder<()> {
                    ResizeRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_cols: impl ::planus::WriteAsDefault<u16, u16>,
                    field_rows: impl ::planus::WriteAsDefault<u16, u16>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_cols = field_cols.prepare(builder, &0);
                    let prepared_rows = field_rows.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<12> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(1);
                    }
                    if prepared_cols.is_some() {
                        table_writer.write_entry::<u16>(2);
                    }
                    if prepared_rows.is_some() {
                        table_writer.write_entry::<u16>(3);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                            if let ::core::option::Option::Some(prepared_cols) = prepared_cols {
                                object_writer.write::<_, _, 2>(&prepared_cols);
                            }
                            if let ::core::option::Option::Some(prepared_rows) = prepared_rows {
                                object_writer.write::<_, _, 2>(&prepared_rows);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<ResizeRequest>> for ResizeRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<ResizeRequest>> for ResizeRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ResizeRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<ResizeRequest> for ResizeRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeRequest> {
                    ResizeRequest::create(
                        builder,
                        self.rpc_id,
                        &self.session_id,
                        self.cols,
                        self.rows,
                    )
                }
            }

            /// Builder for serializing an instance of the [ResizeRequest] type.
            ///
            /// Can be created using the [ResizeRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct ResizeRequestBuilder<State>(State);

            impl ResizeRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](ResizeRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> ResizeRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    ResizeRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](ResizeRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> ResizeRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> ResizeRequestBuilder<(T0,)> {
                /// Setter for the [`session_id` field](ResizeRequest#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T1>(self, value: T1) -> ResizeRequestBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0,) = self.0;
                    ResizeRequestBuilder((v0, value))
                }

                /// Sets the [`session_id` field](ResizeRequest#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> ResizeRequestBuilder<(T0, ())> {
                    self.session_id(())
                }
            }

            impl<T0, T1> ResizeRequestBuilder<(T0, T1)> {
                /// Setter for the [`cols` field](ResizeRequest#structfield.cols).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn cols<T2>(self, value: T2) -> ResizeRequestBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsDefault<u16, u16>,
                {
                    let (v0, v1) = self.0;
                    ResizeRequestBuilder((v0, v1, value))
                }

                /// Sets the [`cols` field](ResizeRequest#structfield.cols) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn cols_as_default(
                    self,
                ) -> ResizeRequestBuilder<(T0, T1, ::planus::DefaultValue)> {
                    self.cols(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2> ResizeRequestBuilder<(T0, T1, T2)> {
                /// Setter for the [`rows` field](ResizeRequest#structfield.rows).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rows<T3>(self, value: T3) -> ResizeRequestBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsDefault<u16, u16>,
                {
                    let (v0, v1, v2) = self.0;
                    ResizeRequestBuilder((v0, v1, v2, value))
                }

                /// Sets the [`rows` field](ResizeRequest#structfield.rows) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rows_as_default(
                    self,
                ) -> ResizeRequestBuilder<(T0, T1, T2, ::planus::DefaultValue)> {
                    self.rows(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3> ResizeRequestBuilder<(T0, T1, T2, T3)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [ResizeRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeRequest>
                where
                    Self: ::planus::WriteAsOffset<ResizeRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<u16, u16>,
                    T3: ::planus::WriteAsDefault<u16, u16>,
                > ::planus::WriteAs<::planus::Offset<ResizeRequest>>
                for ResizeRequestBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<ResizeRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<u16, u16>,
                    T3: ::planus::WriteAsDefault<u16, u16>,
                > ::planus::WriteAsOptional<::planus::Offset<ResizeRequest>>
                for ResizeRequestBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<ResizeRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ResizeRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<u16, u16>,
                    T3: ::planus::WriteAsDefault<u16, u16>,
                > ::planus::WriteAsOffset<ResizeRequest>
                for ResizeRequestBuilder<(T0, T1, T2, T3)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeRequest> {
                    let (v0, v1, v2, v3) = &self.0;
                    ResizeRequest::create(builder, v0, v1, v2, v3)
                }
            }

            /// Reference to a deserialized [ResizeRequest].
            #[derive(Copy, Clone)]
            pub struct ResizeRequestRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> ResizeRequestRef<'a> {
                /// Getter for the [`rpc_id` field](ResizeRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "ResizeRequest", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`session_id` field](ResizeRequest#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(1, "ResizeRequest", "session_id")
                }

                /// Getter for the [`cols` field](ResizeRequest#structfield.cols).
                #[inline]
                pub fn cols(&self) -> ::planus::Result<u16> {
                    ::core::result::Result::Ok(
                        self.0.access(2, "ResizeRequest", "cols")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`rows` field](ResizeRequest#structfield.rows).
                #[inline]
                pub fn rows(&self) -> ::planus::Result<u16> {
                    ::core::result::Result::Ok(
                        self.0.access(3, "ResizeRequest", "rows")?.unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for ResizeRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("ResizeRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.field("cols", &self.cols());
                    f.field("rows", &self.rows());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<ResizeRequestRef<'a>> for ResizeRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: ResizeRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        cols: ::core::convert::TryInto::try_into(value.cols()?)?,
                        rows: ::core::convert::TryInto::try_into(value.rows()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for ResizeRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for ResizeRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[ResizeRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<ResizeRequest>> for ResizeRequest {
                type Value = ::planus::Offset<ResizeRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<ResizeRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for ResizeRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[ResizeRequestRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `ResizeResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `ResizeResponse` in the file `schema/asmux.fbs:109`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct ResizeResponse {
                /// The field `rpc_id` in the table `ResizeResponse`
                pub rpc_id: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for ResizeResponse {
                fn default() -> Self {
                    Self { rpc_id: 0 }
                }
            }

            impl ResizeResponse {
                /// Creates a [ResizeResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> ResizeResponseBuilder<()> {
                    ResizeResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<6> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<ResizeResponse>> for ResizeResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<ResizeResponse>> for ResizeResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ResizeResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<ResizeResponse> for ResizeResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeResponse> {
                    ResizeResponse::create(builder, self.rpc_id)
                }
            }

            /// Builder for serializing an instance of the [ResizeResponse] type.
            ///
            /// Can be created using the [ResizeResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct ResizeResponseBuilder<State>(State);

            impl ResizeResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](ResizeResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> ResizeResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    ResizeResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](ResizeResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> ResizeResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> ResizeResponseBuilder<(T0,)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [ResizeResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeResponse>
                where
                    Self: ::planus::WriteAsOffset<ResizeResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAs<::planus::Offset<ResizeResponse>>
                for ResizeResponseBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<ResizeResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAsOptional<::planus::Offset<ResizeResponse>>
                for ResizeResponseBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<ResizeResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ResizeResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>> ::planus::WriteAsOffset<ResizeResponse>
                for ResizeResponseBuilder<(T0,)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ResizeResponse> {
                    let (v0,) = &self.0;
                    ResizeResponse::create(builder, v0)
                }
            }

            /// Reference to a deserialized [ResizeResponse].
            #[derive(Copy, Clone)]
            pub struct ResizeResponseRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> ResizeResponseRef<'a> {
                /// Getter for the [`rpc_id` field](ResizeResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "ResizeResponse", "rpc_id")?.unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for ResizeResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("ResizeResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<ResizeResponseRef<'a>> for ResizeResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: ResizeResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for ResizeResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for ResizeResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[ResizeResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<ResizeResponse>> for ResizeResponse {
                type Value = ::planus::Offset<ResizeResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<ResizeResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for ResizeResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[ResizeResponseRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `ReadBufferRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `ReadBufferRequest` in the file `schema/asmux.fbs:111`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct ReadBufferRequest {
                /// The field `rpc_id` in the table `ReadBufferRequest`
                pub rpc_id: u64,
                /// The field `session_id` in the table `ReadBufferRequest`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `from_cursor` in the table `ReadBufferRequest`
                pub from_cursor: u64,
                /// The field `max_bytes` in the table `ReadBufferRequest`
                pub max_bytes: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for ReadBufferRequest {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        session_id: ::core::default::Default::default(),
                        from_cursor: 0,
                        max_bytes: 0,
                    }
                }
            }

            impl ReadBufferRequest {
                /// Creates a [ReadBufferRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> ReadBufferRequestBuilder<()> {
                    ReadBufferRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_from_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                    field_max_bytes: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_from_cursor = field_from_cursor.prepare(builder, &0);
                    let prepared_max_bytes = field_max_bytes.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<12> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_from_cursor.is_some() {
                        table_writer.write_entry::<u64>(2);
                    }
                    if prepared_max_bytes.is_some() {
                        table_writer.write_entry::<u64>(3);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_from_cursor) =
                                prepared_from_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_from_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_max_bytes) =
                                prepared_max_bytes
                            {
                                object_writer.write::<_, _, 8>(&prepared_max_bytes);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<ReadBufferRequest>> for ReadBufferRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<ReadBufferRequest>> for ReadBufferRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ReadBufferRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<ReadBufferRequest> for ReadBufferRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferRequest> {
                    ReadBufferRequest::create(
                        builder,
                        self.rpc_id,
                        &self.session_id,
                        self.from_cursor,
                        self.max_bytes,
                    )
                }
            }

            /// Builder for serializing an instance of the [ReadBufferRequest] type.
            ///
            /// Can be created using the [ReadBufferRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct ReadBufferRequestBuilder<State>(State);

            impl ReadBufferRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](ReadBufferRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> ReadBufferRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    ReadBufferRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](ReadBufferRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(
                    self,
                ) -> ReadBufferRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> ReadBufferRequestBuilder<(T0,)> {
                /// Setter for the [`session_id` field](ReadBufferRequest#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T1>(self, value: T1) -> ReadBufferRequestBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0,) = self.0;
                    ReadBufferRequestBuilder((v0, value))
                }

                /// Sets the [`session_id` field](ReadBufferRequest#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> ReadBufferRequestBuilder<(T0, ())> {
                    self.session_id(())
                }
            }

            impl<T0, T1> ReadBufferRequestBuilder<(T0, T1)> {
                /// Setter for the [`from_cursor` field](ReadBufferRequest#structfield.from_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn from_cursor<T2>(self, value: T2) -> ReadBufferRequestBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1) = self.0;
                    ReadBufferRequestBuilder((v0, v1, value))
                }

                /// Sets the [`from_cursor` field](ReadBufferRequest#structfield.from_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn from_cursor_as_default(
                    self,
                ) -> ReadBufferRequestBuilder<(T0, T1, ::planus::DefaultValue)> {
                    self.from_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2> ReadBufferRequestBuilder<(T0, T1, T2)> {
                /// Setter for the [`max_bytes` field](ReadBufferRequest#structfield.max_bytes).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn max_bytes<T3>(self, value: T3) -> ReadBufferRequestBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1, v2) = self.0;
                    ReadBufferRequestBuilder((v0, v1, v2, value))
                }

                /// Sets the [`max_bytes` field](ReadBufferRequest#structfield.max_bytes) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn max_bytes_as_default(
                    self,
                ) -> ReadBufferRequestBuilder<(T0, T1, T2, ::planus::DefaultValue)>
                {
                    self.max_bytes(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3> ReadBufferRequestBuilder<(T0, T1, T2, T3)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [ReadBufferRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferRequest>
                where
                    Self: ::planus::WriteAsOffset<ReadBufferRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<u64, u64>,
                    T3: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAs<::planus::Offset<ReadBufferRequest>>
                for ReadBufferRequestBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<ReadBufferRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<u64, u64>,
                    T3: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOptional<::planus::Offset<ReadBufferRequest>>
                for ReadBufferRequestBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<ReadBufferRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ReadBufferRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<u64, u64>,
                    T3: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOffset<ReadBufferRequest>
                for ReadBufferRequestBuilder<(T0, T1, T2, T3)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferRequest> {
                    let (v0, v1, v2, v3) = &self.0;
                    ReadBufferRequest::create(builder, v0, v1, v2, v3)
                }
            }

            /// Reference to a deserialized [ReadBufferRequest].
            #[derive(Copy, Clone)]
            pub struct ReadBufferRequestRef<'a>(
                #[allow(dead_code)] ::planus::table_reader::Table<'a>,
            );

            impl<'a> ReadBufferRequestRef<'a> {
                /// Getter for the [`rpc_id` field](ReadBufferRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(0, "ReadBufferRequest", "rpc_id")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`session_id` field](ReadBufferRequest#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(1, "ReadBufferRequest", "session_id")
                }

                /// Getter for the [`from_cursor` field](ReadBufferRequest#structfield.from_cursor).
                #[inline]
                pub fn from_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(2, "ReadBufferRequest", "from_cursor")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`max_bytes` field](ReadBufferRequest#structfield.max_bytes).
                #[inline]
                pub fn max_bytes(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(3, "ReadBufferRequest", "max_bytes")?
                            .unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for ReadBufferRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("ReadBufferRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.field("from_cursor", &self.from_cursor());
                    f.field("max_bytes", &self.max_bytes());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<ReadBufferRequestRef<'a>> for ReadBufferRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: ReadBufferRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        from_cursor: ::core::convert::TryInto::try_into(value.from_cursor()?)?,
                        max_bytes: ::core::convert::TryInto::try_into(value.max_bytes()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for ReadBufferRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for ReadBufferRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[ReadBufferRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<ReadBufferRequest>> for ReadBufferRequest {
                type Value = ::planus::Offset<ReadBufferRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<ReadBufferRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for ReadBufferRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[ReadBufferRequestRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `ReadBufferResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `ReadBufferResponse` in the file `schema/asmux.fbs:117`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct ReadBufferResponse {
                /// The field `rpc_id` in the table `ReadBufferResponse`
                pub rpc_id: u64,
                /// The field `from_cursor` in the table `ReadBufferResponse`
                pub from_cursor: u64,
                /// The field `head_cursor` in the table `ReadBufferResponse`
                pub head_cursor: u64,
                /// The field `data` in the table `ReadBufferResponse`
                pub data: ::core::option::Option<::planus::alloc::vec::Vec<u8>>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for ReadBufferResponse {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        from_cursor: 0,
                        head_cursor: 0,
                        data: ::core::default::Default::default(),
                    }
                }
            }

            impl ReadBufferResponse {
                /// Creates a [ReadBufferResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> ReadBufferResponseBuilder<()> {
                    ReadBufferResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_from_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                    field_head_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                    field_data: impl ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_from_cursor = field_from_cursor.prepare(builder, &0);
                    let prepared_head_cursor = field_head_cursor.prepare(builder, &0);
                    let prepared_data = field_data.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<12> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_from_cursor.is_some() {
                        table_writer.write_entry::<u64>(1);
                    }
                    if prepared_head_cursor.is_some() {
                        table_writer.write_entry::<u64>(2);
                    }
                    if prepared_data.is_some() {
                        table_writer.write_entry::<::planus::Offset<[u8]>>(3);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_from_cursor) =
                                prepared_from_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_from_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_head_cursor) =
                                prepared_head_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_head_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_data) = prepared_data {
                                object_writer.write::<_, _, 4>(&prepared_data);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<ReadBufferResponse>> for ReadBufferResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<ReadBufferResponse>> for ReadBufferResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ReadBufferResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<ReadBufferResponse> for ReadBufferResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferResponse> {
                    ReadBufferResponse::create(
                        builder,
                        self.rpc_id,
                        self.from_cursor,
                        self.head_cursor,
                        &self.data,
                    )
                }
            }

            /// Builder for serializing an instance of the [ReadBufferResponse] type.
            ///
            /// Can be created using the [ReadBufferResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct ReadBufferResponseBuilder<State>(State);

            impl ReadBufferResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](ReadBufferResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> ReadBufferResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    ReadBufferResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](ReadBufferResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(
                    self,
                ) -> ReadBufferResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> ReadBufferResponseBuilder<(T0,)> {
                /// Setter for the [`from_cursor` field](ReadBufferResponse#structfield.from_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn from_cursor<T1>(self, value: T1) -> ReadBufferResponseBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0,) = self.0;
                    ReadBufferResponseBuilder((v0, value))
                }

                /// Sets the [`from_cursor` field](ReadBufferResponse#structfield.from_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn from_cursor_as_default(
                    self,
                ) -> ReadBufferResponseBuilder<(T0, ::planus::DefaultValue)> {
                    self.from_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1> ReadBufferResponseBuilder<(T0, T1)> {
                /// Setter for the [`head_cursor` field](ReadBufferResponse#structfield.head_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor<T2>(self, value: T2) -> ReadBufferResponseBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1) = self.0;
                    ReadBufferResponseBuilder((v0, v1, value))
                }

                /// Sets the [`head_cursor` field](ReadBufferResponse#structfield.head_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor_as_default(
                    self,
                ) -> ReadBufferResponseBuilder<(T0, T1, ::planus::DefaultValue)> {
                    self.head_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2> ReadBufferResponseBuilder<(T0, T1, T2)> {
                /// Setter for the [`data` field](ReadBufferResponse#structfield.data).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn data<T3>(self, value: T3) -> ReadBufferResponseBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                {
                    let (v0, v1, v2) = self.0;
                    ReadBufferResponseBuilder((v0, v1, v2, value))
                }

                /// Sets the [`data` field](ReadBufferResponse#structfield.data) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn data_as_null(self) -> ReadBufferResponseBuilder<(T0, T1, T2, ())> {
                    self.data(())
                }
            }

            impl<T0, T1, T2, T3> ReadBufferResponseBuilder<(T0, T1, T2, T3)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [ReadBufferResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferResponse>
                where
                    Self: ::planus::WriteAsOffset<ReadBufferResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<u64, u64>,
                    T2: ::planus::WriteAsDefault<u64, u64>,
                    T3: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                > ::planus::WriteAs<::planus::Offset<ReadBufferResponse>>
                for ReadBufferResponseBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<ReadBufferResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<u64, u64>,
                    T2: ::planus::WriteAsDefault<u64, u64>,
                    T3: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                > ::planus::WriteAsOptional<::planus::Offset<ReadBufferResponse>>
                for ReadBufferResponseBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<ReadBufferResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<ReadBufferResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<u64, u64>,
                    T2: ::planus::WriteAsDefault<u64, u64>,
                    T3: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                > ::planus::WriteAsOffset<ReadBufferResponse>
                for ReadBufferResponseBuilder<(T0, T1, T2, T3)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<ReadBufferResponse> {
                    let (v0, v1, v2, v3) = &self.0;
                    ReadBufferResponse::create(builder, v0, v1, v2, v3)
                }
            }

            /// Reference to a deserialized [ReadBufferResponse].
            #[derive(Copy, Clone)]
            pub struct ReadBufferResponseRef<'a>(
                #[allow(dead_code)] ::planus::table_reader::Table<'a>,
            );

            impl<'a> ReadBufferResponseRef<'a> {
                /// Getter for the [`rpc_id` field](ReadBufferResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(0, "ReadBufferResponse", "rpc_id")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`from_cursor` field](ReadBufferResponse#structfield.from_cursor).
                #[inline]
                pub fn from_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(1, "ReadBufferResponse", "from_cursor")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`head_cursor` field](ReadBufferResponse#structfield.head_cursor).
                #[inline]
                pub fn head_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(2, "ReadBufferResponse", "head_cursor")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`data` field](ReadBufferResponse#structfield.data).
                #[inline]
                pub fn data(&self) -> ::planus::Result<::core::option::Option<&'a [u8]>> {
                    self.0.access(3, "ReadBufferResponse", "data")
                }
            }

            impl<'a> ::core::fmt::Debug for ReadBufferResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("ReadBufferResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.field("from_cursor", &self.from_cursor());
                    f.field("head_cursor", &self.head_cursor());
                    if let ::core::option::Option::Some(field_data) = self.data().transpose() {
                        f.field("data", &field_data);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<ReadBufferResponseRef<'a>> for ReadBufferResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: ReadBufferResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        from_cursor: ::core::convert::TryInto::try_into(value.from_cursor()?)?,
                        head_cursor: ::core::convert::TryInto::try_into(value.head_cursor()?)?,
                        data: value.data()?.map(|v| v.to_vec()),
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for ReadBufferResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for ReadBufferResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[ReadBufferResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<ReadBufferResponse>> for ReadBufferResponse {
                type Value = ::planus::Offset<ReadBufferResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<ReadBufferResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for ReadBufferResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[ReadBufferResponseRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `AttachRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `AttachRequest` in the file `schema/asmux.fbs:124`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct AttachRequest {
                /// The field `rpc_id` in the table `AttachRequest`
                pub rpc_id: u64,
                /// The field `session_id` in the table `AttachRequest`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `mode` in the table `AttachRequest`
                pub mode: self::AttachMode,
                /// The field `from_cursor` in the table `AttachRequest`
                pub from_cursor: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for AttachRequest {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        session_id: ::core::default::Default::default(),
                        mode: self::AttachMode::FromCursor,
                        from_cursor: 0,
                    }
                }
            }

            impl AttachRequest {
                /// Creates a [AttachRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> AttachRequestBuilder<()> {
                    AttachRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_mode: impl ::planus::WriteAsDefault<self::AttachMode, self::AttachMode>,
                    field_from_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_mode = field_mode.prepare(builder, &self::AttachMode::FromCursor);
                    let prepared_from_cursor = field_from_cursor.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<12> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_from_cursor.is_some() {
                        table_writer.write_entry::<u64>(3);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(1);
                    }
                    if prepared_mode.is_some() {
                        table_writer.write_entry::<self::AttachMode>(2);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_from_cursor) =
                                prepared_from_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_from_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                            if let ::core::option::Option::Some(prepared_mode) = prepared_mode {
                                object_writer.write::<_, _, 1>(&prepared_mode);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<AttachRequest>> for AttachRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<AttachRequest>> for AttachRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<AttachRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<AttachRequest> for AttachRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachRequest> {
                    AttachRequest::create(
                        builder,
                        self.rpc_id,
                        &self.session_id,
                        self.mode,
                        self.from_cursor,
                    )
                }
            }

            /// Builder for serializing an instance of the [AttachRequest] type.
            ///
            /// Can be created using the [AttachRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct AttachRequestBuilder<State>(State);

            impl AttachRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](AttachRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> AttachRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    AttachRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](AttachRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> AttachRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> AttachRequestBuilder<(T0,)> {
                /// Setter for the [`session_id` field](AttachRequest#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T1>(self, value: T1) -> AttachRequestBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0,) = self.0;
                    AttachRequestBuilder((v0, value))
                }

                /// Sets the [`session_id` field](AttachRequest#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> AttachRequestBuilder<(T0, ())> {
                    self.session_id(())
                }
            }

            impl<T0, T1> AttachRequestBuilder<(T0, T1)> {
                /// Setter for the [`mode` field](AttachRequest#structfield.mode).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn mode<T2>(self, value: T2) -> AttachRequestBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsDefault<self::AttachMode, self::AttachMode>,
                {
                    let (v0, v1) = self.0;
                    AttachRequestBuilder((v0, v1, value))
                }

                /// Sets the [`mode` field](AttachRequest#structfield.mode) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn mode_as_default(
                    self,
                ) -> AttachRequestBuilder<(T0, T1, ::planus::DefaultValue)> {
                    self.mode(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2> AttachRequestBuilder<(T0, T1, T2)> {
                /// Setter for the [`from_cursor` field](AttachRequest#structfield.from_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn from_cursor<T3>(self, value: T3) -> AttachRequestBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1, v2) = self.0;
                    AttachRequestBuilder((v0, v1, v2, value))
                }

                /// Sets the [`from_cursor` field](AttachRequest#structfield.from_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn from_cursor_as_default(
                    self,
                ) -> AttachRequestBuilder<(T0, T1, T2, ::planus::DefaultValue)> {
                    self.from_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3> AttachRequestBuilder<(T0, T1, T2, T3)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [AttachRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachRequest>
                where
                    Self: ::planus::WriteAsOffset<AttachRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<self::AttachMode, self::AttachMode>,
                    T3: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAs<::planus::Offset<AttachRequest>>
                for AttachRequestBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<AttachRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<self::AttachMode, self::AttachMode>,
                    T3: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOptional<::planus::Offset<AttachRequest>>
                for AttachRequestBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<AttachRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<AttachRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T2: ::planus::WriteAsDefault<self::AttachMode, self::AttachMode>,
                    T3: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOffset<AttachRequest>
                for AttachRequestBuilder<(T0, T1, T2, T3)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachRequest> {
                    let (v0, v1, v2, v3) = &self.0;
                    AttachRequest::create(builder, v0, v1, v2, v3)
                }
            }

            /// Reference to a deserialized [AttachRequest].
            #[derive(Copy, Clone)]
            pub struct AttachRequestRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> AttachRequestRef<'a> {
                /// Getter for the [`rpc_id` field](AttachRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "AttachRequest", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`session_id` field](AttachRequest#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(1, "AttachRequest", "session_id")
                }

                /// Getter for the [`mode` field](AttachRequest#structfield.mode).
                #[inline]
                pub fn mode(&self) -> ::planus::Result<self::AttachMode> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(2, "AttachRequest", "mode")?
                            .unwrap_or(self::AttachMode::FromCursor),
                    )
                }

                /// Getter for the [`from_cursor` field](AttachRequest#structfield.from_cursor).
                #[inline]
                pub fn from_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(3, "AttachRequest", "from_cursor")?
                            .unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for AttachRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("AttachRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.field("mode", &self.mode());
                    f.field("from_cursor", &self.from_cursor());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<AttachRequestRef<'a>> for AttachRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: AttachRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        mode: ::core::convert::TryInto::try_into(value.mode()?)?,
                        from_cursor: ::core::convert::TryInto::try_into(value.from_cursor()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for AttachRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for AttachRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[AttachRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<AttachRequest>> for AttachRequest {
                type Value = ::planus::Offset<AttachRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<AttachRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for AttachRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[AttachRequestRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `AttachResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `AttachResponse` in the file `schema/asmux.fbs:130`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct AttachResponse {
                /// The field `rpc_id` in the table `AttachResponse`
                pub rpc_id: u64,
                /// The field `head_cursor` in the table `AttachResponse`
                pub head_cursor: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for AttachResponse {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        head_cursor: 0,
                    }
                }
            }

            impl AttachResponse {
                /// Creates a [AttachResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> AttachResponseBuilder<()> {
                    AttachResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_head_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_head_cursor = field_head_cursor.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<8> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_head_cursor.is_some() {
                        table_writer.write_entry::<u64>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_head_cursor) =
                                prepared_head_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_head_cursor);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<AttachResponse>> for AttachResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<AttachResponse>> for AttachResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<AttachResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<AttachResponse> for AttachResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachResponse> {
                    AttachResponse::create(builder, self.rpc_id, self.head_cursor)
                }
            }

            /// Builder for serializing an instance of the [AttachResponse] type.
            ///
            /// Can be created using the [AttachResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct AttachResponseBuilder<State>(State);

            impl AttachResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](AttachResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> AttachResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    AttachResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](AttachResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> AttachResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> AttachResponseBuilder<(T0,)> {
                /// Setter for the [`head_cursor` field](AttachResponse#structfield.head_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor<T1>(self, value: T1) -> AttachResponseBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0,) = self.0;
                    AttachResponseBuilder((v0, value))
                }

                /// Sets the [`head_cursor` field](AttachResponse#structfield.head_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor_as_default(
                    self,
                ) -> AttachResponseBuilder<(T0, ::planus::DefaultValue)> {
                    self.head_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1> AttachResponseBuilder<(T0, T1)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [AttachResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachResponse>
                where
                    Self: ::planus::WriteAsOffset<AttachResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAs<::planus::Offset<AttachResponse>>
                for AttachResponseBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<AttachResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOptional<::planus::Offset<AttachResponse>>
                for AttachResponseBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<AttachResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<AttachResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOffset<AttachResponse> for AttachResponseBuilder<(T0, T1)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<AttachResponse> {
                    let (v0, v1) = &self.0;
                    AttachResponse::create(builder, v0, v1)
                }
            }

            /// Reference to a deserialized [AttachResponse].
            #[derive(Copy, Clone)]
            pub struct AttachResponseRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> AttachResponseRef<'a> {
                /// Getter for the [`rpc_id` field](AttachResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "AttachResponse", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`head_cursor` field](AttachResponse#structfield.head_cursor).
                #[inline]
                pub fn head_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(1, "AttachResponse", "head_cursor")?
                            .unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for AttachResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("AttachResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.field("head_cursor", &self.head_cursor());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<AttachResponseRef<'a>> for AttachResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: AttachResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        head_cursor: ::core::convert::TryInto::try_into(value.head_cursor()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for AttachResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for AttachResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[AttachResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<AttachResponse>> for AttachResponse {
                type Value = ::planus::Offset<AttachResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<AttachResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for AttachResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[AttachResponseRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `DetachRequest` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `DetachRequest` in the file `schema/asmux.fbs:135`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct DetachRequest {
                /// The field `rpc_id` in the table `DetachRequest`
                pub rpc_id: u64,
                /// The field `session_id` in the table `DetachRequest`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for DetachRequest {
                fn default() -> Self {
                    Self {
                        rpc_id: 0,
                        session_id: ::core::default::Default::default(),
                    }
                }
            }

            impl DetachRequest {
                /// Creates a [DetachRequestBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> DetachRequestBuilder<()> {
                    DetachRequestBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);
                    let prepared_session_id = field_session_id.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<8> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<DetachRequest>> for DetachRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<DetachRequest>> for DetachRequest {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<DetachRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<DetachRequest> for DetachRequest {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachRequest> {
                    DetachRequest::create(builder, self.rpc_id, &self.session_id)
                }
            }

            /// Builder for serializing an instance of the [DetachRequest] type.
            ///
            /// Can be created using the [DetachRequest::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct DetachRequestBuilder<State>(State);

            impl DetachRequestBuilder<()> {
                /// Setter for the [`rpc_id` field](DetachRequest#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> DetachRequestBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    DetachRequestBuilder((value,))
                }

                /// Sets the [`rpc_id` field](DetachRequest#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> DetachRequestBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> DetachRequestBuilder<(T0,)> {
                /// Setter for the [`session_id` field](DetachRequest#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T1>(self, value: T1) -> DetachRequestBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    let (v0,) = self.0;
                    DetachRequestBuilder((v0, value))
                }

                /// Sets the [`session_id` field](DetachRequest#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> DetachRequestBuilder<(T0, ())> {
                    self.session_id(())
                }
            }

            impl<T0, T1> DetachRequestBuilder<(T0, T1)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [DetachRequest].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachRequest>
                where
                    Self: ::planus::WriteAsOffset<DetachRequest>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAs<::planus::Offset<DetachRequest>>
                for DetachRequestBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<DetachRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachRequest> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOptional<::planus::Offset<DetachRequest>>
                for DetachRequestBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<DetachRequest>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<DetachRequest>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsDefault<u64, u64>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                > ::planus::WriteAsOffset<DetachRequest> for DetachRequestBuilder<(T0, T1)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachRequest> {
                    let (v0, v1) = &self.0;
                    DetachRequest::create(builder, v0, v1)
                }
            }

            /// Reference to a deserialized [DetachRequest].
            #[derive(Copy, Clone)]
            pub struct DetachRequestRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> DetachRequestRef<'a> {
                /// Getter for the [`rpc_id` field](DetachRequest#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "DetachRequest", "rpc_id")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`session_id` field](DetachRequest#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(1, "DetachRequest", "session_id")
                }
            }

            impl<'a> ::core::fmt::Debug for DetachRequestRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("DetachRequestRef");
                    f.field("rpc_id", &self.rpc_id());
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<DetachRequestRef<'a>> for DetachRequest {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: DetachRequestRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for DetachRequestRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for DetachRequestRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[DetachRequestRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<DetachRequest>> for DetachRequest {
                type Value = ::planus::Offset<DetachRequest>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<DetachRequest>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for DetachRequestRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[DetachRequestRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `DetachResponse` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `DetachResponse` in the file `schema/asmux.fbs:136`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct DetachResponse {
                /// The field `rpc_id` in the table `DetachResponse`
                pub rpc_id: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for DetachResponse {
                fn default() -> Self {
                    Self { rpc_id: 0 }
                }
            }

            impl DetachResponse {
                /// Creates a [DetachResponseBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> DetachResponseBuilder<()> {
                    DetachResponseBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_rpc_id: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_rpc_id = field_rpc_id.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<6> =
                        ::core::default::Default::default();
                    if prepared_rpc_id.is_some() {
                        table_writer.write_entry::<u64>(0);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_rpc_id) = prepared_rpc_id {
                                object_writer.write::<_, _, 8>(&prepared_rpc_id);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<DetachResponse>> for DetachResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<DetachResponse>> for DetachResponse {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<DetachResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<DetachResponse> for DetachResponse {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachResponse> {
                    DetachResponse::create(builder, self.rpc_id)
                }
            }

            /// Builder for serializing an instance of the [DetachResponse] type.
            ///
            /// Can be created using the [DetachResponse::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct DetachResponseBuilder<State>(State);

            impl DetachResponseBuilder<()> {
                /// Setter for the [`rpc_id` field](DetachResponse#structfield.rpc_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id<T0>(self, value: T0) -> DetachResponseBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<u64, u64>,
                {
                    DetachResponseBuilder((value,))
                }

                /// Sets the [`rpc_id` field](DetachResponse#structfield.rpc_id) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn rpc_id_as_default(self) -> DetachResponseBuilder<(::planus::DefaultValue,)> {
                    self.rpc_id(::planus::DefaultValue)
                }
            }

            impl<T0> DetachResponseBuilder<(T0,)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [DetachResponse].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachResponse>
                where
                    Self: ::planus::WriteAsOffset<DetachResponse>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAs<::planus::Offset<DetachResponse>>
                for DetachResponseBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<DetachResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachResponse> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>>
                ::planus::WriteAsOptional<::planus::Offset<DetachResponse>>
                for DetachResponseBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<DetachResponse>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<DetachResponse>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<T0: ::planus::WriteAsDefault<u64, u64>> ::planus::WriteAsOffset<DetachResponse>
                for DetachResponseBuilder<(T0,)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<DetachResponse> {
                    let (v0,) = &self.0;
                    DetachResponse::create(builder, v0)
                }
            }

            /// Reference to a deserialized [DetachResponse].
            #[derive(Copy, Clone)]
            pub struct DetachResponseRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> DetachResponseRef<'a> {
                /// Getter for the [`rpc_id` field](DetachResponse#structfield.rpc_id).
                #[inline]
                pub fn rpc_id(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "DetachResponse", "rpc_id")?.unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for DetachResponseRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("DetachResponseRef");
                    f.field("rpc_id", &self.rpc_id());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<DetachResponseRef<'a>> for DetachResponse {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: DetachResponseRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        rpc_id: ::core::convert::TryInto::try_into(value.rpc_id()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for DetachResponseRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for DetachResponseRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[DetachResponseRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<DetachResponse>> for DetachResponse {
                type Value = ::planus::Offset<DetachResponse>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<DetachResponse>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for DetachResponseRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[DetachResponseRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `SessionExited` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `SessionExited` in the file `schema/asmux.fbs:138`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct SessionExited {
                /// The field `session_id` in the table `SessionExited`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `exit_code` in the table `SessionExited`
                pub exit_code: i32,
                /// The field `exit_signal` in the table `SessionExited`
                pub exit_signal: i32,
                /// The field `head_cursor` in the table `SessionExited`
                pub head_cursor: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for SessionExited {
                fn default() -> Self {
                    Self {
                        session_id: ::core::default::Default::default(),
                        exit_code: 0,
                        exit_signal: 0,
                        head_cursor: 0,
                    }
                }
            }

            impl SessionExited {
                /// Creates a [SessionExitedBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> SessionExitedBuilder<()> {
                    SessionExitedBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_exit_code: impl ::planus::WriteAsDefault<i32, i32>,
                    field_exit_signal: impl ::planus::WriteAsDefault<i32, i32>,
                    field_head_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_exit_code = field_exit_code.prepare(builder, &0);
                    let prepared_exit_signal = field_exit_signal.prepare(builder, &0);
                    let prepared_head_cursor = field_head_cursor.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<12> =
                        ::core::default::Default::default();
                    if prepared_head_cursor.is_some() {
                        table_writer.write_entry::<u64>(3);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(0);
                    }
                    if prepared_exit_code.is_some() {
                        table_writer.write_entry::<i32>(1);
                    }
                    if prepared_exit_signal.is_some() {
                        table_writer.write_entry::<i32>(2);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_head_cursor) =
                                prepared_head_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_head_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                            if let ::core::option::Option::Some(prepared_exit_code) =
                                prepared_exit_code
                            {
                                object_writer.write::<_, _, 4>(&prepared_exit_code);
                            }
                            if let ::core::option::Option::Some(prepared_exit_signal) =
                                prepared_exit_signal
                            {
                                object_writer.write::<_, _, 4>(&prepared_exit_signal);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<SessionExited>> for SessionExited {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionExited> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<SessionExited>> for SessionExited {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionExited>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<SessionExited> for SessionExited {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionExited> {
                    SessionExited::create(
                        builder,
                        &self.session_id,
                        self.exit_code,
                        self.exit_signal,
                        self.head_cursor,
                    )
                }
            }

            /// Builder for serializing an instance of the [SessionExited] type.
            ///
            /// Can be created using the [SessionExited::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct SessionExitedBuilder<State>(State);

            impl SessionExitedBuilder<()> {
                /// Setter for the [`session_id` field](SessionExited#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T0>(self, value: T0) -> SessionExitedBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    SessionExitedBuilder((value,))
                }

                /// Sets the [`session_id` field](SessionExited#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> SessionExitedBuilder<((),)> {
                    self.session_id(())
                }
            }

            impl<T0> SessionExitedBuilder<(T0,)> {
                /// Setter for the [`exit_code` field](SessionExited#structfield.exit_code).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn exit_code<T1>(self, value: T1) -> SessionExitedBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsDefault<i32, i32>,
                {
                    let (v0,) = self.0;
                    SessionExitedBuilder((v0, value))
                }

                /// Sets the [`exit_code` field](SessionExited#structfield.exit_code) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn exit_code_as_default(
                    self,
                ) -> SessionExitedBuilder<(T0, ::planus::DefaultValue)> {
                    self.exit_code(::planus::DefaultValue)
                }
            }

            impl<T0, T1> SessionExitedBuilder<(T0, T1)> {
                /// Setter for the [`exit_signal` field](SessionExited#structfield.exit_signal).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn exit_signal<T2>(self, value: T2) -> SessionExitedBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsDefault<i32, i32>,
                {
                    let (v0, v1) = self.0;
                    SessionExitedBuilder((v0, v1, value))
                }

                /// Sets the [`exit_signal` field](SessionExited#structfield.exit_signal) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn exit_signal_as_default(
                    self,
                ) -> SessionExitedBuilder<(T0, T1, ::planus::DefaultValue)> {
                    self.exit_signal(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2> SessionExitedBuilder<(T0, T1, T2)> {
                /// Setter for the [`head_cursor` field](SessionExited#structfield.head_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor<T3>(self, value: T3) -> SessionExitedBuilder<(T0, T1, T2, T3)>
                where
                    T3: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1, v2) = self.0;
                    SessionExitedBuilder((v0, v1, v2, value))
                }

                /// Sets the [`head_cursor` field](SessionExited#structfield.head_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor_as_default(
                    self,
                ) -> SessionExitedBuilder<(T0, T1, T2, ::planus::DefaultValue)> {
                    self.head_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2, T3> SessionExitedBuilder<(T0, T1, T2, T3)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [SessionExited].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionExited>
                where
                    Self: ::planus::WriteAsOffset<SessionExited>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<i32, i32>,
                    T2: ::planus::WriteAsDefault<i32, i32>,
                    T3: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAs<::planus::Offset<SessionExited>>
                for SessionExitedBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<SessionExited>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionExited> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<i32, i32>,
                    T2: ::planus::WriteAsDefault<i32, i32>,
                    T3: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOptional<::planus::Offset<SessionExited>>
                for SessionExitedBuilder<(T0, T1, T2, T3)>
            {
                type Prepared = ::planus::Offset<SessionExited>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionExited>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<i32, i32>,
                    T2: ::planus::WriteAsDefault<i32, i32>,
                    T3: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOffset<SessionExited>
                for SessionExitedBuilder<(T0, T1, T2, T3)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionExited> {
                    let (v0, v1, v2, v3) = &self.0;
                    SessionExited::create(builder, v0, v1, v2, v3)
                }
            }

            /// Reference to a deserialized [SessionExited].
            #[derive(Copy, Clone)]
            pub struct SessionExitedRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> SessionExitedRef<'a> {
                /// Getter for the [`session_id` field](SessionExited#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(0, "SessionExited", "session_id")
                }

                /// Getter for the [`exit_code` field](SessionExited#structfield.exit_code).
                #[inline]
                pub fn exit_code(&self) -> ::planus::Result<i32> {
                    ::core::result::Result::Ok(
                        self.0.access(1, "SessionExited", "exit_code")?.unwrap_or(0),
                    )
                }

                /// Getter for the [`exit_signal` field](SessionExited#structfield.exit_signal).
                #[inline]
                pub fn exit_signal(&self) -> ::planus::Result<i32> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(2, "SessionExited", "exit_signal")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`head_cursor` field](SessionExited#structfield.head_cursor).
                #[inline]
                pub fn head_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(3, "SessionExited", "head_cursor")?
                            .unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for SessionExitedRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("SessionExitedRef");
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.field("exit_code", &self.exit_code());
                    f.field("exit_signal", &self.exit_signal());
                    f.field("head_cursor", &self.head_cursor());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<SessionExitedRef<'a>> for SessionExited {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: SessionExitedRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        exit_code: ::core::convert::TryInto::try_into(value.exit_code()?)?,
                        exit_signal: ::core::convert::TryInto::try_into(value.exit_signal()?)?,
                        head_cursor: ::core::convert::TryInto::try_into(value.head_cursor()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for SessionExitedRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for SessionExitedRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[SessionExitedRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<SessionExited>> for SessionExited {
                type Value = ::planus::Offset<SessionExited>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<SessionExited>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for SessionExitedRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[SessionExitedRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `SessionDetached` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `SessionDetached` in the file `schema/asmux.fbs:145`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct SessionDetached {
                /// The field `session_id` in the table `SessionDetached`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `reason` in the table `SessionDetached`
                pub reason: self::DetachReason,
                /// The field `last_cursor` in the table `SessionDetached`
                pub last_cursor: u64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for SessionDetached {
                fn default() -> Self {
                    Self {
                        session_id: ::core::default::Default::default(),
                        reason: self::DetachReason::Superseded,
                        last_cursor: 0,
                    }
                }
            }

            impl SessionDetached {
                /// Creates a [SessionDetachedBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> SessionDetachedBuilder<()> {
                    SessionDetachedBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_reason: impl ::planus::WriteAsDefault<self::DetachReason, self::DetachReason>,
                    field_last_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_reason =
                        field_reason.prepare(builder, &self::DetachReason::Superseded);
                    let prepared_last_cursor = field_last_cursor.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<10> =
                        ::core::default::Default::default();
                    if prepared_last_cursor.is_some() {
                        table_writer.write_entry::<u64>(2);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(0);
                    }
                    if prepared_reason.is_some() {
                        table_writer.write_entry::<self::DetachReason>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_last_cursor) =
                                prepared_last_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_last_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                            if let ::core::option::Option::Some(prepared_reason) = prepared_reason {
                                object_writer.write::<_, _, 1>(&prepared_reason);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<SessionDetached>> for SessionDetached {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionDetached> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<SessionDetached>> for SessionDetached {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionDetached>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<SessionDetached> for SessionDetached {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionDetached> {
                    SessionDetached::create(
                        builder,
                        &self.session_id,
                        self.reason,
                        self.last_cursor,
                    )
                }
            }

            /// Builder for serializing an instance of the [SessionDetached] type.
            ///
            /// Can be created using the [SessionDetached::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct SessionDetachedBuilder<State>(State);

            impl SessionDetachedBuilder<()> {
                /// Setter for the [`session_id` field](SessionDetached#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T0>(self, value: T0) -> SessionDetachedBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    SessionDetachedBuilder((value,))
                }

                /// Sets the [`session_id` field](SessionDetached#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> SessionDetachedBuilder<((),)> {
                    self.session_id(())
                }
            }

            impl<T0> SessionDetachedBuilder<(T0,)> {
                /// Setter for the [`reason` field](SessionDetached#structfield.reason).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn reason<T1>(self, value: T1) -> SessionDetachedBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsDefault<self::DetachReason, self::DetachReason>,
                {
                    let (v0,) = self.0;
                    SessionDetachedBuilder((v0, value))
                }

                /// Sets the [`reason` field](SessionDetached#structfield.reason) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn reason_as_default(
                    self,
                ) -> SessionDetachedBuilder<(T0, ::planus::DefaultValue)> {
                    self.reason(::planus::DefaultValue)
                }
            }

            impl<T0, T1> SessionDetachedBuilder<(T0, T1)> {
                /// Setter for the [`last_cursor` field](SessionDetached#structfield.last_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn last_cursor<T2>(self, value: T2) -> SessionDetachedBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0, v1) = self.0;
                    SessionDetachedBuilder((v0, v1, value))
                }

                /// Sets the [`last_cursor` field](SessionDetached#structfield.last_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn last_cursor_as_default(
                    self,
                ) -> SessionDetachedBuilder<(T0, T1, ::planus::DefaultValue)> {
                    self.last_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1, T2> SessionDetachedBuilder<(T0, T1, T2)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [SessionDetached].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionDetached>
                where
                    Self: ::planus::WriteAsOffset<SessionDetached>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<self::DetachReason, self::DetachReason>,
                    T2: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAs<::planus::Offset<SessionDetached>>
                for SessionDetachedBuilder<(T0, T1, T2)>
            {
                type Prepared = ::planus::Offset<SessionDetached>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionDetached> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<self::DetachReason, self::DetachReason>,
                    T2: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOptional<::planus::Offset<SessionDetached>>
                for SessionDetachedBuilder<(T0, T1, T2)>
            {
                type Prepared = ::planus::Offset<SessionDetached>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionDetached>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<self::DetachReason, self::DetachReason>,
                    T2: ::planus::WriteAsDefault<u64, u64>,
                > ::planus::WriteAsOffset<SessionDetached>
                for SessionDetachedBuilder<(T0, T1, T2)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionDetached> {
                    let (v0, v1, v2) = &self.0;
                    SessionDetached::create(builder, v0, v1, v2)
                }
            }

            /// Reference to a deserialized [SessionDetached].
            #[derive(Copy, Clone)]
            pub struct SessionDetachedRef<'a>(
                #[allow(dead_code)] ::planus::table_reader::Table<'a>,
            );

            impl<'a> SessionDetachedRef<'a> {
                /// Getter for the [`session_id` field](SessionDetached#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(0, "SessionDetached", "session_id")
                }

                /// Getter for the [`reason` field](SessionDetached#structfield.reason).
                #[inline]
                pub fn reason(&self) -> ::planus::Result<self::DetachReason> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(1, "SessionDetached", "reason")?
                            .unwrap_or(self::DetachReason::Superseded),
                    )
                }

                /// Getter for the [`last_cursor` field](SessionDetached#structfield.last_cursor).
                #[inline]
                pub fn last_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(2, "SessionDetached", "last_cursor")?
                            .unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for SessionDetachedRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("SessionDetachedRef");
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.field("reason", &self.reason());
                    f.field("last_cursor", &self.last_cursor());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<SessionDetachedRef<'a>> for SessionDetached {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: SessionDetachedRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        reason: ::core::convert::TryInto::try_into(value.reason()?)?,
                        last_cursor: ::core::convert::TryInto::try_into(value.last_cursor()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for SessionDetachedRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for SessionDetachedRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[SessionDetachedRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<SessionDetached>> for SessionDetached {
                type Value = ::planus::Offset<SessionDetached>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<SessionDetached>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for SessionDetachedRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[SessionDetachedRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `SessionInput` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `SessionInput` in the file `schema/asmux.fbs:151`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct SessionInput {
                /// The field `session_id` in the table `SessionInput`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `data` in the table `SessionInput`
                pub data: ::core::option::Option<::planus::alloc::vec::Vec<u8>>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for SessionInput {
                fn default() -> Self {
                    Self {
                        session_id: ::core::default::Default::default(),
                        data: ::core::default::Default::default(),
                    }
                }
            }

            impl SessionInput {
                /// Creates a [SessionInputBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> SessionInputBuilder<()> {
                    SessionInputBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_data: impl ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                ) -> ::planus::Offset<Self> {
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_data = field_data.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<8> =
                        ::core::default::Default::default();
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(0);
                    }
                    if prepared_data.is_some() {
                        table_writer.write_entry::<::planus::Offset<[u8]>>(1);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                            if let ::core::option::Option::Some(prepared_data) = prepared_data {
                                object_writer.write::<_, _, 4>(&prepared_data);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<SessionInput>> for SessionInput {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionInput> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<SessionInput>> for SessionInput {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionInput>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<SessionInput> for SessionInput {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionInput> {
                    SessionInput::create(builder, &self.session_id, &self.data)
                }
            }

            /// Builder for serializing an instance of the [SessionInput] type.
            ///
            /// Can be created using the [SessionInput::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct SessionInputBuilder<State>(State);

            impl SessionInputBuilder<()> {
                /// Setter for the [`session_id` field](SessionInput#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T0>(self, value: T0) -> SessionInputBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    SessionInputBuilder((value,))
                }

                /// Sets the [`session_id` field](SessionInput#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> SessionInputBuilder<((),)> {
                    self.session_id(())
                }
            }

            impl<T0> SessionInputBuilder<(T0,)> {
                /// Setter for the [`data` field](SessionInput#structfield.data).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn data<T1>(self, value: T1) -> SessionInputBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                {
                    let (v0,) = self.0;
                    SessionInputBuilder((v0, value))
                }

                /// Sets the [`data` field](SessionInput#structfield.data) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn data_as_null(self) -> SessionInputBuilder<(T0, ())> {
                    self.data(())
                }
            }

            impl<T0, T1> SessionInputBuilder<(T0, T1)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [SessionInput].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionInput>
                where
                    Self: ::planus::WriteAsOffset<SessionInput>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                > ::planus::WriteAs<::planus::Offset<SessionInput>>
                for SessionInputBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<SessionInput>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionInput> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                > ::planus::WriteAsOptional<::planus::Offset<SessionInput>>
                for SessionInputBuilder<(T0, T1)>
            {
                type Prepared = ::planus::Offset<SessionInput>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionInput>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                > ::planus::WriteAsOffset<SessionInput> for SessionInputBuilder<(T0, T1)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionInput> {
                    let (v0, v1) = &self.0;
                    SessionInput::create(builder, v0, v1)
                }
            }

            /// Reference to a deserialized [SessionInput].
            #[derive(Copy, Clone)]
            pub struct SessionInputRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> SessionInputRef<'a> {
                /// Getter for the [`session_id` field](SessionInput#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(0, "SessionInput", "session_id")
                }

                /// Getter for the [`data` field](SessionInput#structfield.data).
                #[inline]
                pub fn data(&self) -> ::planus::Result<::core::option::Option<&'a [u8]>> {
                    self.0.access(1, "SessionInput", "data")
                }
            }

            impl<'a> ::core::fmt::Debug for SessionInputRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("SessionInputRef");
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    if let ::core::option::Option::Some(field_data) = self.data().transpose() {
                        f.field("data", &field_data);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<SessionInputRef<'a>> for SessionInput {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: SessionInputRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        data: value.data()?.map(|v| v.to_vec()),
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for SessionInputRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for SessionInputRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[SessionInputRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<SessionInput>> for SessionInput {
                type Value = ::planus::Offset<SessionInput>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<SessionInput>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for SessionInputRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[SessionInputRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `SessionOutput` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `SessionOutput` in the file `schema/asmux.fbs:155`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct SessionOutput {
                /// The field `session_id` in the table `SessionOutput`
                pub session_id: ::core::option::Option<::planus::alloc::string::String>,
                /// The field `head_cursor` in the table `SessionOutput`
                pub head_cursor: u64,
                /// The field `data` in the table `SessionOutput`
                pub data: ::core::option::Option<::planus::alloc::vec::Vec<u8>>,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for SessionOutput {
                fn default() -> Self {
                    Self {
                        session_id: ::core::default::Default::default(),
                        head_cursor: 0,
                        data: ::core::default::Default::default(),
                    }
                }
            }

            impl SessionOutput {
                /// Creates a [SessionOutputBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> SessionOutputBuilder<()> {
                    SessionOutputBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_session_id: impl ::planus::WriteAsOptional<
                        ::planus::Offset<::core::primitive::str>,
                    >,
                    field_head_cursor: impl ::planus::WriteAsDefault<u64, u64>,
                    field_data: impl ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                ) -> ::planus::Offset<Self> {
                    let prepared_session_id = field_session_id.prepare(builder);
                    let prepared_head_cursor = field_head_cursor.prepare(builder, &0);
                    let prepared_data = field_data.prepare(builder);

                    let mut table_writer: ::planus::table_writer::TableWriter<10> =
                        ::core::default::Default::default();
                    if prepared_head_cursor.is_some() {
                        table_writer.write_entry::<u64>(1);
                    }
                    if prepared_session_id.is_some() {
                        table_writer.write_entry::<::planus::Offset<str>>(0);
                    }
                    if prepared_data.is_some() {
                        table_writer.write_entry::<::planus::Offset<[u8]>>(2);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_head_cursor) =
                                prepared_head_cursor
                            {
                                object_writer.write::<_, _, 8>(&prepared_head_cursor);
                            }
                            if let ::core::option::Option::Some(prepared_session_id) =
                                prepared_session_id
                            {
                                object_writer.write::<_, _, 4>(&prepared_session_id);
                            }
                            if let ::core::option::Option::Some(prepared_data) = prepared_data {
                                object_writer.write::<_, _, 4>(&prepared_data);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<SessionOutput>> for SessionOutput {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionOutput> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<SessionOutput>> for SessionOutput {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionOutput>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<SessionOutput> for SessionOutput {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionOutput> {
                    SessionOutput::create(builder, &self.session_id, self.head_cursor, &self.data)
                }
            }

            /// Builder for serializing an instance of the [SessionOutput] type.
            ///
            /// Can be created using the [SessionOutput::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct SessionOutputBuilder<State>(State);

            impl SessionOutputBuilder<()> {
                /// Setter for the [`session_id` field](SessionOutput#structfield.session_id).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id<T0>(self, value: T0) -> SessionOutputBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                {
                    SessionOutputBuilder((value,))
                }

                /// Sets the [`session_id` field](SessionOutput#structfield.session_id) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn session_id_as_null(self) -> SessionOutputBuilder<((),)> {
                    self.session_id(())
                }
            }

            impl<T0> SessionOutputBuilder<(T0,)> {
                /// Setter for the [`head_cursor` field](SessionOutput#structfield.head_cursor).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor<T1>(self, value: T1) -> SessionOutputBuilder<(T0, T1)>
                where
                    T1: ::planus::WriteAsDefault<u64, u64>,
                {
                    let (v0,) = self.0;
                    SessionOutputBuilder((v0, value))
                }

                /// Sets the [`head_cursor` field](SessionOutput#structfield.head_cursor) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn head_cursor_as_default(
                    self,
                ) -> SessionOutputBuilder<(T0, ::planus::DefaultValue)> {
                    self.head_cursor(::planus::DefaultValue)
                }
            }

            impl<T0, T1> SessionOutputBuilder<(T0, T1)> {
                /// Setter for the [`data` field](SessionOutput#structfield.data).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn data<T2>(self, value: T2) -> SessionOutputBuilder<(T0, T1, T2)>
                where
                    T2: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                {
                    let (v0, v1) = self.0;
                    SessionOutputBuilder((v0, v1, value))
                }

                /// Sets the [`data` field](SessionOutput#structfield.data) to null.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn data_as_null(self) -> SessionOutputBuilder<(T0, T1, ())> {
                    self.data(())
                }
            }

            impl<T0, T1, T2> SessionOutputBuilder<(T0, T1, T2)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [SessionOutput].
                #[inline]
                pub fn finish(
                    self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionOutput>
                where
                    Self: ::planus::WriteAsOffset<SessionOutput>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<u64, u64>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                > ::planus::WriteAs<::planus::Offset<SessionOutput>>
                for SessionOutputBuilder<(T0, T1, T2)>
            {
                type Prepared = ::planus::Offset<SessionOutput>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionOutput> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<u64, u64>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                > ::planus::WriteAsOptional<::planus::Offset<SessionOutput>>
                for SessionOutputBuilder<(T0, T1, T2)>
            {
                type Prepared = ::planus::Offset<SessionOutput>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<SessionOutput>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<
                    T0: ::planus::WriteAsOptional<::planus::Offset<::core::primitive::str>>,
                    T1: ::planus::WriteAsDefault<u64, u64>,
                    T2: ::planus::WriteAsOptional<::planus::Offset<[u8]>>,
                > ::planus::WriteAsOffset<SessionOutput> for SessionOutputBuilder<(T0, T1, T2)>
            {
                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::planus::Offset<SessionOutput> {
                    let (v0, v1, v2) = &self.0;
                    SessionOutput::create(builder, v0, v1, v2)
                }
            }

            /// Reference to a deserialized [SessionOutput].
            #[derive(Copy, Clone)]
            pub struct SessionOutputRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> SessionOutputRef<'a> {
                /// Getter for the [`session_id` field](SessionOutput#structfield.session_id).
                #[inline]
                pub fn session_id(
                    &self,
                ) -> ::planus::Result<::core::option::Option<&'a ::core::primitive::str>>
                {
                    self.0.access(0, "SessionOutput", "session_id")
                }

                /// Getter for the [`head_cursor` field](SessionOutput#structfield.head_cursor).
                #[inline]
                pub fn head_cursor(&self) -> ::planus::Result<u64> {
                    ::core::result::Result::Ok(
                        self.0
                            .access(1, "SessionOutput", "head_cursor")?
                            .unwrap_or(0),
                    )
                }

                /// Getter for the [`data` field](SessionOutput#structfield.data).
                #[inline]
                pub fn data(&self) -> ::planus::Result<::core::option::Option<&'a [u8]>> {
                    self.0.access(2, "SessionOutput", "data")
                }
            }

            impl<'a> ::core::fmt::Debug for SessionOutputRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("SessionOutputRef");
                    if let ::core::option::Option::Some(field_session_id) =
                        self.session_id().transpose()
                    {
                        f.field("session_id", &field_session_id);
                    }
                    f.field("head_cursor", &self.head_cursor());
                    if let ::core::option::Option::Some(field_data) = self.data().transpose() {
                        f.field("data", &field_data);
                    }
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<SessionOutputRef<'a>> for SessionOutput {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: SessionOutputRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        session_id: value.session_id()?.map(::core::convert::Into::into),
                        head_cursor: ::core::convert::TryInto::try_into(value.head_cursor()?)?,
                        data: value.data()?.map(|v| v.to_vec()),
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for SessionOutputRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for SessionOutputRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[SessionOutputRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<SessionOutput>> for SessionOutput {
                type Value = ::planus::Offset<SessionOutput>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<SessionOutput>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for SessionOutputRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[SessionOutputRef]", "read_as_root", 0)
                    })
                }
            }

            /// The table `Heartbeat` in the namespace `asmux.wire`
            ///
            /// Generated from these locations:
            /// * Table `Heartbeat` in the file `schema/asmux.fbs:161`
            #[derive(
                Clone,
                Debug,
                PartialEq,
                PartialOrd,
                Eq,
                Ord,
                Hash,
                ::serde::Serialize,
                ::serde::Deserialize,
            )]
            pub struct Heartbeat {
                /// The field `unix_ms` in the table `Heartbeat`
                pub unix_ms: i64,
            }

            #[allow(clippy::derivable_impls)]
            impl ::core::default::Default for Heartbeat {
                fn default() -> Self {
                    Self { unix_ms: 0 }
                }
            }

            impl Heartbeat {
                /// Creates a [HeartbeatBuilder] for serializing an instance of this table.
                #[inline]
                pub fn builder() -> HeartbeatBuilder<()> {
                    HeartbeatBuilder(())
                }

                #[allow(clippy::too_many_arguments)]
                pub fn create(
                    builder: &mut ::planus::Builder,
                    field_unix_ms: impl ::planus::WriteAsDefault<i64, i64>,
                ) -> ::planus::Offset<Self> {
                    let prepared_unix_ms = field_unix_ms.prepare(builder, &0);

                    let mut table_writer: ::planus::table_writer::TableWriter<6> =
                        ::core::default::Default::default();
                    if prepared_unix_ms.is_some() {
                        table_writer.write_entry::<i64>(0);
                    }

                    unsafe {
                        table_writer.finish(builder, |object_writer| {
                            if let ::core::option::Option::Some(prepared_unix_ms) = prepared_unix_ms
                            {
                                object_writer.write::<_, _, 8>(&prepared_unix_ms);
                            }
                        });
                    }
                    builder.current_offset()
                }
            }

            impl ::planus::WriteAs<::planus::Offset<Heartbeat>> for Heartbeat {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Heartbeat> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl ::planus::WriteAsOptional<::planus::Offset<Heartbeat>> for Heartbeat {
                type Prepared = ::planus::Offset<Self>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<Heartbeat>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl ::planus::WriteAsOffset<Heartbeat> for Heartbeat {
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Heartbeat> {
                    Heartbeat::create(builder, self.unix_ms)
                }
            }

            /// Builder for serializing an instance of the [Heartbeat] type.
            ///
            /// Can be created using the [Heartbeat::builder] method.
            #[derive(Debug)]
            #[must_use]
            pub struct HeartbeatBuilder<State>(State);

            impl HeartbeatBuilder<()> {
                /// Setter for the [`unix_ms` field](Heartbeat#structfield.unix_ms).
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn unix_ms<T0>(self, value: T0) -> HeartbeatBuilder<(T0,)>
                where
                    T0: ::planus::WriteAsDefault<i64, i64>,
                {
                    HeartbeatBuilder((value,))
                }

                /// Sets the [`unix_ms` field](Heartbeat#structfield.unix_ms) to the default value.
                #[inline]
                #[allow(clippy::type_complexity)]
                pub fn unix_ms_as_default(self) -> HeartbeatBuilder<(::planus::DefaultValue,)> {
                    self.unix_ms(::planus::DefaultValue)
                }
            }

            impl<T0> HeartbeatBuilder<(T0,)> {
                /// Finish writing the builder to get an [Offset](::planus::Offset) to a serialized [Heartbeat].
                #[inline]
                pub fn finish(self, builder: &mut ::planus::Builder) -> ::planus::Offset<Heartbeat>
                where
                    Self: ::planus::WriteAsOffset<Heartbeat>,
                {
                    ::planus::WriteAsOffset::prepare(&self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<i64, i64>>
                ::planus::WriteAs<::planus::Offset<Heartbeat>> for HeartbeatBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<Heartbeat>;

                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Heartbeat> {
                    ::planus::WriteAsOffset::prepare(self, builder)
                }
            }

            impl<T0: ::planus::WriteAsDefault<i64, i64>>
                ::planus::WriteAsOptional<::planus::Offset<Heartbeat>> for HeartbeatBuilder<(T0,)>
            {
                type Prepared = ::planus::Offset<Heartbeat>;

                #[inline]
                fn prepare(
                    &self,
                    builder: &mut ::planus::Builder,
                ) -> ::core::option::Option<::planus::Offset<Heartbeat>> {
                    ::core::option::Option::Some(::planus::WriteAsOffset::prepare(self, builder))
                }
            }

            impl<T0: ::planus::WriteAsDefault<i64, i64>> ::planus::WriteAsOffset<Heartbeat>
                for HeartbeatBuilder<(T0,)>
            {
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> ::planus::Offset<Heartbeat> {
                    let (v0,) = &self.0;
                    Heartbeat::create(builder, v0)
                }
            }

            /// Reference to a deserialized [Heartbeat].
            #[derive(Copy, Clone)]
            pub struct HeartbeatRef<'a>(#[allow(dead_code)] ::planus::table_reader::Table<'a>);

            impl<'a> HeartbeatRef<'a> {
                /// Getter for the [`unix_ms` field](Heartbeat#structfield.unix_ms).
                #[inline]
                pub fn unix_ms(&self) -> ::planus::Result<i64> {
                    ::core::result::Result::Ok(
                        self.0.access(0, "Heartbeat", "unix_ms")?.unwrap_or(0),
                    )
                }
            }

            impl<'a> ::core::fmt::Debug for HeartbeatRef<'a> {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    let mut f = f.debug_struct("HeartbeatRef");
                    f.field("unix_ms", &self.unix_ms());
                    f.finish()
                }
            }

            impl<'a> ::core::convert::TryFrom<HeartbeatRef<'a>> for Heartbeat {
                type Error = ::planus::Error;

                #[allow(unreachable_code)]
                fn try_from(value: HeartbeatRef<'a>) -> ::planus::Result<Self> {
                    ::core::result::Result::Ok(Self {
                        unix_ms: ::core::convert::TryInto::try_into(value.unix_ms()?)?,
                    })
                }
            }

            impl<'a> ::planus::TableRead<'a> for HeartbeatRef<'a> {
                #[inline]
                fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::core::result::Result<Self, ::planus::errors::ErrorKind> {
                    ::core::result::Result::Ok(Self(::planus::table_reader::Table::from_buffer(
                        buffer, offset,
                    )?))
                }
            }

            impl<'a> ::planus::VectorReadInner<'a> for HeartbeatRef<'a> {
                type Error = ::planus::Error;
                const STRIDE: usize = 4;

                unsafe fn from_buffer(
                    buffer: ::planus::SliceWithStartOffset<'a>,
                    offset: usize,
                ) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(buffer, offset).map_err(|error_kind| {
                        error_kind.with_error_location(
                            "[HeartbeatRef]",
                            "get",
                            buffer.offset_from_start,
                        )
                    })
                }
            }

            /// # Safety
            /// The planus compiler generates implementations that initialize
            /// the bytes in `write_values`.
            unsafe impl ::planus::VectorWrite<::planus::Offset<Heartbeat>> for Heartbeat {
                type Value = ::planus::Offset<Heartbeat>;
                const STRIDE: usize = 4;
                #[inline]
                fn prepare(&self, builder: &mut ::planus::Builder) -> Self::Value {
                    ::planus::WriteAs::prepare(self, builder)
                }

                #[inline]
                unsafe fn write_values(
                    values: &[::planus::Offset<Heartbeat>],
                    bytes: *mut ::core::mem::MaybeUninit<u8>,
                    buffer_position: u32,
                ) {
                    let bytes = bytes as *mut [::core::mem::MaybeUninit<u8>; 4];
                    for (i, v) in ::core::iter::Iterator::enumerate(values.iter()) {
                        ::planus::WriteAsPrimitive::write(
                            v,
                            ::planus::Cursor::new(unsafe { &mut *bytes.add(i) }),
                            buffer_position - (Self::STRIDE * i) as u32,
                        );
                    }
                }
            }

            impl<'a> ::planus::ReadAsRoot<'a> for HeartbeatRef<'a> {
                fn read_as_root(slice: &'a [u8]) -> ::planus::Result<Self> {
                    ::planus::TableRead::from_buffer(
                        ::planus::SliceWithStartOffset {
                            buffer: slice,
                            offset_from_start: 0,
                        },
                        0,
                    )
                    .map_err(|error_kind| {
                        error_kind.with_error_location("[HeartbeatRef]", "read_as_root", 0)
                    })
                }
            }
        }
    }
}
