extern crate proc_macro;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use proc_macro_error::{abort, proc_macro_error};
use quote::ToTokens;
use syn::{parse_macro_input, FnArg, Ident, Item, ItemFn, ItemMod, ReturnType};

type TokenStream2 = proc_macro2::TokenStream;

struct FunctionsMod<'a> {
    name: &'a Ident,
    mu_functions: Vec<ItemFn>,
    other_items: Vec<&'a Item>,
}

#[proc_macro_error]
#[proc_macro_attribute]
pub fn mu_functions(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let r#mod = parse_macro_input!(item as ItemMod);

    let r#mod = read_function_mod(&r#mod);

    generate_mu_functions_wrapper(&r#mod).into()
}

fn read_function_mod(r#mod: &ItemMod) -> FunctionsMod<'_> {
    let name = &r#mod.ident;

    let mut mu_functions = vec![];
    let mut other_items = vec![];

    for item in &r#mod
        .content
        .as_ref()
        .unwrap_or_else(|| abort!(r#mod, "mu_functions module must contain at least one item"))
        .1
    {
        match item {
            Item::Fn(r#fn) => match remove_mu_function_attribute(r#fn) {
                Some(f) => mu_functions.push(f),
                None => other_items.push(item),
            },
            _ => other_items.push(item),
        }
    }

    if mu_functions.is_empty() {
        abort!(
            r#mod,
            "No mu functions were found. You should declare at least one mu function; otherwise, your code will have no entrypoint.";
            tip = "To declare a mu function, decorate an fn with #[mu_function]"
        );
    }

    FunctionsMod {
        name,
        mu_functions,
        other_items,
    }
}

fn remove_mu_function_attribute(r#fn: &ItemFn) -> Option<ItemFn> {
    for (i, attr) in r#fn.attrs.iter().enumerate() {
        if let Some(ident) = attr.path.get_ident() {
            if ident == "mu_function" {
                let mut f = r#fn.clone();
                f.attrs.remove(i);
                return Some(f);
            }
        }
    }

    None
}

fn struct_ident(ident: &Ident) -> Ident {
    Ident::new(&format!("{ident}Impl"), ident.span())
}

fn generate_mu_functions_wrapper(r#mod: &FunctionsMod) -> TokenStream2 {
    let struct_name = struct_ident(r#mod.name);

    let main_fn = generate_main_fn(r#mod);
    let context_factory = generate_context_factory(r#mod);
    let module = generate_module(r#mod);

    quote!(
        #main_fn

        #[allow(non_camel_case_types)]
        struct #struct_name;

        #context_factory

        #module
    )
}

fn generate_main_fn(r#mod: &FunctionsMod) -> TokenStream2 {
    let struct_name = struct_ident(r#mod.name);
    // TODO: fully qualify types once their namespaces are decided
    quote!(
        fn main() {
            ::musdk::MuContext::run::<#struct_name>();
        }
    )
}

fn generate_context_factory_function(r#mod: &FunctionsMod) -> TokenStream2 {
    let mut fns = vec![];
    let mut tuples = vec![];

    for f in &r#mod.mu_functions {
        let name = &f.sig.ident;
        let fn_name = Ident::new(format!("fn_{}", name).as_str(), name.span());
        let invoker_name = Ident::new(format!("_invoker_{}", name).as_str(), name.span());
        fns.push(quote!(
            let #fn_name: ::musdk::MuFunction = ::std::rc::Rc::new(|c, r| #invoker_name(c, r));
        ));
        let name_str = name.to_string();
        tuples.push(quote!((#name_str.to_string(), #fn_name)));
    }

    quote!(
        pub(super) fn _context_factory_create_context() -> MuContext {
            #(#fns)*
            let functions = [
                #(#tuples),*
            ]
            .into_iter()
            .collect::<::std::collections::HashMap<::std::string::String, ::musdk::MuFunction>>();

            ::musdk::MuContext::new(functions)
        }
    )
}

fn generate_context_factory(r#mod: &FunctionsMod) -> TokenStream2 {
    let struct_name = struct_ident(r#mod.name);
    let mod_name = r#mod.name;
    // TODO: fully qualify types once their namespaces are decided
    quote!(
        impl ::musdk::ContextFactory for #struct_name {
            fn create_context() -> ::musdk::MuContext {
                #mod_name::_context_factory_create_context()
            }
        }
    )
}

fn generate_module(r#mod: &FunctionsMod) -> TokenStream2 {
    let invokers = generate_invokers(r#mod);
    let context_factory = generate_context_factory_function(r#mod);
    let FunctionsMod {
        ref name,
        ref mu_functions,
        ref other_items,
    } = r#mod;

    quote!(mod #name {
        #context_factory

        #(#invokers)*

        #(#mu_functions)*

        #(#other_items)*
    })
}

fn generate_invokers(r#mod: &FunctionsMod) -> Vec<TokenStream2> {
    let mut result = vec![];

    for f in &r#mod.mu_functions {
        let name = &f.sig.ident;
        let invoker_name = Ident::new(format!("_invoker_{}", name).as_str(), name.span());

        let (generics, context_lifetime) = {
            match f.sig.generics.params.iter().find_map(|g| match g {
                syn::GenericParam::Lifetime(l) => Some(l),
                _ => None,
            }) {
                Some(l) => (f.sig.generics.clone(), l.clone()),
                None => {
                    abort!(f.sig.ident, "mu functions must include a lifetime parameter, used to receive the MuContext by reference")
                }
            }
        };

        let mut input_arg = vec![];
        let mut input_where = vec![];

        for input in f.sig.inputs.iter().skip(1) {
            let pat_type = match input {
                FnArg::Typed(t) => t,
                FnArg::Receiver(_) => {
                    abort!(input, "self arguments are not supported in mu functions")
                }
            };
            let typ = pat_type.ty.as_ref();

            input_arg.push(quote!(
                match <#typ as ::musdk::FromRequest<#context_lifetime>>::from_request(request) {
                    Ok(arg) => arg,
                    Err(err) =>
                        return
                            <<#typ as ::musdk::FromRequest<#context_lifetime>>::Error
                                as ::musdk::IntoResponse<'static>>::into_response(err),
                }
            ));

            input_where.push(quote!(#typ: ::musdk::FromRequest<#context_lifetime>));
        }

        let return_type = match &f.sig.output {
            ReturnType::Default => quote!(()),
            ReturnType::Type(_, typ) => typ.to_token_stream(),
        };

        result.push(quote!(
            fn #invoker_name #generics(
                ctx: &#context_lifetime mut ::musdk::MuContext,
                request: &#context_lifetime ::musdk::Request,
            ) -> ::musdk::Response<'static>
            where
                #(#input_where,)*
                #return_type: ::musdk::IntoResponse<'static>,
            {
                <#return_type as ::musdk::IntoResponse<'static>>::into_response(#name(ctx, #(#input_arg,)*))
            }
        ))
    }

    result
}
