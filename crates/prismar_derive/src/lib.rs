use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
  Attribute, Data, DeriveInput, Field, Fields, GenericArgument, LitStr, Meta,
  PathArguments, Type, parse_macro_input,
};

#[proc_macro_derive(PrismarModel, attributes(prismar))]
pub fn derive_prismar_model(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  expand_prismar_model(input)
    .unwrap_or_else(|err| err.to_compile_error())
    .into()
}

fn expand_prismar_model(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
  let model_name = input.ident;
  let fields = match input.data {
    Data::Struct(data) => match data.fields {
      Fields::Named(fields) => fields.named,
      _ => {
        return Err(syn::Error::new_spanned(
          model_name,
          "PrismarModel only supports structs with named fields",
        ));
      }
    },
    _ => {
      return Err(syn::Error::new_spanned(
        model_name,
        "PrismarModel only supports structs",
      ));
    }
  };

  let filter_name = format_ident!("{}Filter", model_name);

  let mut methods = Vec::new();
  for field in fields {
    methods.extend(generate_methods_for_field(&field)?);
  }

  Ok(quote! {
    #[derive(Debug, Clone, Default, PartialEq)]
    #[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
    pub struct #filter_name {
      inner: ::prismar::ModelFilter,
    }

    impl #filter_name {
      pub fn new() -> Self {
        Self::default()
      }

      pub fn and_where<F>(mut self, build: F) -> Self
      where
        F: FnOnce(Self) -> Self,
      {
        let nested = build(Self::new());
        self.inner = self.inner.and_group(vec![nested.inner]);
        self
      }

      pub fn or_where<F>(mut self, build: F) -> Self
      where
        F: FnOnce(Self) -> Self,
      {
        let nested = build(Self::new());
        self.inner = self.inner.or_group(vec![nested.inner]);
        self
      }

      pub fn and(mut self, other: Self) -> Self {
        self.inner = self.inner.and_group(vec![other.inner]);
        self
      }

      pub fn or(mut self, other: Self) -> Self {
        self.inner = self.inner.or_group(vec![other.inner]);
        self
      }

      pub fn not(mut self, other: Self) -> Self {
        self.inner = self.inner.not_group(other.inner);
        self
      }

      pub fn build(self) -> ::prismar::ModelFilter {
        self.inner
      }

      #(#methods)*
    }

    impl From<#filter_name> for ::prismar::ModelFilter {
      fn from(value: #filter_name) -> Self {
        value.inner
      }
    }

    impl ::prismar::TypedFilter for #filter_name {
      fn into_model_filter(self) -> ::prismar::ModelFilter {
        self.inner
      }
    }
  })
}

fn generate_methods_for_field(
  field: &Field,
) -> syn::Result<Vec<proc_macro2::TokenStream>> {
  let field_ident = field
    .ident
    .clone()
    .ok_or_else(|| syn::Error::new_spanned(field, "Expected named field"))?;
  let where_ident = format_ident!("{}_where", field_ident);
  let field_name = LitStr::new(&field_ident.to_string(), field_ident.span());
  let kind = classify_field(field)?;

  let mut methods = Vec::new();

  match kind.kind {
    FilterKind::String => {
      methods.push(quote! {
        pub fn #field_ident(mut self, op: ::prismar::StringFilter) -> Self {
          self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::String(op));
          self
        }

        pub fn #where_ident<F>(mut self, build: F) -> Self
        where
          F: FnOnce(::prismar::StringOps) -> ::prismar::StringOps,
        {
          let ops = build(::prismar::StringOps::new());
          for op in ops.into_filters() {
            self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::String(op));
          }
          self
        }
      });
    }
    FilterKind::Number => {
      methods.push(quote! {
        pub fn #field_ident(mut self, op: ::prismar::NumberFilter) -> Self {
          self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::Number(op));
          self
        }

        pub fn #where_ident<F>(mut self, build: F) -> Self
        where
          F: FnOnce(::prismar::NumberOps) -> ::prismar::NumberOps,
        {
          let ops = build(::prismar::NumberOps::new());
          for op in ops.into_filters() {
            self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::Number(op));
          }
          self
        }
      });
    }
    FilterKind::Bool => {
      methods.push(quote! {
        pub fn #field_ident(mut self, op: ::prismar::BoolFilter) -> Self {
          self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::Bool(op));
          self
        }

        pub fn #where_ident<F>(mut self, build: F) -> Self
        where
          F: FnOnce(::prismar::BoolOps) -> ::prismar::BoolOps,
        {
          let ops = build(::prismar::BoolOps::new());
          for op in ops.into_filters() {
            self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::Bool(op));
          }
          self
        }
      });
    }
    FilterKind::DateTime => {
      methods.push(quote! {
        pub fn #field_ident(mut self, op: ::prismar::DateTimeFilter) -> Self {
          self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::DateTime(op));
          self
        }

        pub fn #where_ident<F>(mut self, build: F) -> Self
        where
          F: FnOnce(::prismar::DateTimeOps) -> ::prismar::DateTimeOps,
        {
          let ops = build(::prismar::DateTimeOps::new());
          for op in ops.into_filters() {
            self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::DateTime(op));
          }
          self
        }
      });
    }
    FilterKind::Uuid => {
      methods.push(quote! {
        pub fn #field_ident(mut self, op: ::prismar::UuidFilter) -> Self {
          self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::Uuid(op));
          self
        }

        pub fn #where_ident<F>(mut self, build: F) -> Self
        where
          F: FnOnce(::prismar::UuidOps) -> ::prismar::UuidOps,
        {
          let ops = build(::prismar::UuidOps::new());
          for op in ops.into_filters() {
            self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::Uuid(op));
          }
          self
        }
      });
    }
    FilterKind::Json => {
      methods.push(quote! {
        pub fn #field_ident(mut self, op: ::prismar::JsonFilter) -> Self {
          self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::Json(op));
          self
        }

        pub fn #where_ident<F>(mut self, build: F) -> Self
        where
          F: FnOnce(::prismar::JsonOps) -> ::prismar::JsonOps,
        {
          let ops = build(::prismar::JsonOps::new());
          for op in ops.into_filters() {
            self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::Json(op));
          }
          self
        }
      });
    }
    FilterKind::Relation => {
      let some = format_ident!("{}_some", field_ident);
      let every = format_ident!("{}_every", field_ident);
      let none = format_ident!("{}_none", field_ident);
      let is = format_ident!("{}_is", field_ident);
      let is_not = format_ident!("{}_is_not", field_ident);
      methods.push(quote! {
        pub fn #some<T: ::prismar::TypedFilter>(mut self, filter: T) -> Self {
          self.inner = self.inner.relation(
            #field_name,
            ::prismar::RelationFilterOp::Some,
            filter.into_model_filter(),
          );
          self
        }

        pub fn #every<T: ::prismar::TypedFilter>(mut self, filter: T) -> Self {
          self.inner = self.inner.relation(
            #field_name,
            ::prismar::RelationFilterOp::Every,
            filter.into_model_filter(),
          );
          self
        }

        pub fn #none<T: ::prismar::TypedFilter>(mut self, filter: T) -> Self {
          self.inner = self.inner.relation(
            #field_name,
            ::prismar::RelationFilterOp::None,
            filter.into_model_filter(),
          );
          self
        }

        pub fn #is<T: ::prismar::TypedFilter>(mut self, filter: T) -> Self {
          self.inner = self.inner.relation(
            #field_name,
            ::prismar::RelationFilterOp::Is,
            filter.into_model_filter(),
          );
          self
        }

        pub fn #is_not<T: ::prismar::TypedFilter>(mut self, filter: T) -> Self {
          self.inner = self.inner.relation(
            #field_name,
            ::prismar::RelationFilterOp::IsNot,
            filter.into_model_filter(),
          );
          self
        }
      });
    }
  }

  if kind.nullable {
    let null_fn = format_ident!("{}_is_null", field_ident);
    let not_null_fn = format_ident!("{}_is_not_null", field_ident);
    methods.push(quote! {
      pub fn #null_fn(mut self) -> Self {
        self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::Null);
        self
      }

      pub fn #not_null_fn(mut self) -> Self {
        self.inner = self.inner.predicate(#field_name, ::prismar::FieldFilter::NotNull);
        self
      }
    });
  }

  Ok(methods)
}

#[derive(Debug, Clone, Copy)]
struct ClassifiedField {
  kind: FilterKind,
  nullable: bool,
}

#[derive(Debug, Clone, Copy)]
enum FilterKind {
  String,
  Number,
  Bool,
  DateTime,
  Uuid,
  Json,
  Relation,
}

fn classify_field(field: &Field) -> syn::Result<ClassifiedField> {
  let mut nullable = false;
  let mut ty = &field.ty;

  if let Some(inner) = option_inner(ty) {
    nullable = true;
    ty = inner;
  }

  if has_prismar_flag(&field.attrs, "relation") {
    return Ok(ClassifiedField {
      kind: FilterKind::Relation,
      nullable,
    });
  }

  if has_prismar_flag(&field.attrs, "json") {
    return Ok(ClassifiedField {
      kind: FilterKind::Json,
      nullable,
    });
  }

  let kind = if is_type_named(ty, &["String"]) {
    FilterKind::String
  } else if is_type_named(ty, &["bool"]) {
    FilterKind::Bool
  } else if is_type_named(
    ty,
    &[
      "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32",
      "u64", "u128", "usize", "f32", "f64",
    ],
  ) {
    FilterKind::Number
  } else if is_type_named(ty, &["Uuid"]) {
    FilterKind::Uuid
  } else if is_type_named(ty, &["NaiveDateTime", "DateTime"]) {
    FilterKind::DateTime
  } else if is_type_named(ty, &["Value"]) {
    FilterKind::Json
  } else {
    FilterKind::String
  };

  Ok(ClassifiedField { kind, nullable })
}

fn option_inner(ty: &Type) -> Option<&Type> {
  let Type::Path(path) = ty else { return None };
  let segment = path.path.segments.last()?;
  if segment.ident != "Option" {
    return None;
  }
  let PathArguments::AngleBracketed(args) = &segment.arguments else {
    return None;
  };
  let first = args.args.first()?;
  if let GenericArgument::Type(ty) = first {
    Some(ty)
  } else {
    None
  }
}

fn is_type_named(ty: &Type, names: &[&str]) -> bool {
  let Type::Path(path) = ty else { return false };
  path
    .path
    .segments
    .last()
    .map(|segment| names.iter().any(|name| segment.ident == *name))
    .unwrap_or(false)
}

fn has_prismar_flag(attrs: &[Attribute], flag: &str) -> bool {
  for attr in attrs {
    if !attr.path().is_ident("prismar") {
      continue;
    }
    let Meta::List(list) = &attr.meta else {
      continue;
    };
    let mut found = false;
    let _ = list.parse_nested_meta(|meta| {
      if meta.path.is_ident(flag) {
        found = true;
      }
      Ok(())
    });
    if found {
      return true;
    }
  }
  false
}
