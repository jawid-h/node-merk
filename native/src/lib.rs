#[macro_use]
extern crate neon;
extern crate merk;

use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Mutex;
use merk::{Merk, Op};
use neon::prelude::*;

pub struct MerkHandle {
    store: Rc<Mutex<Merk>>
}

pub struct Batch {
    ops: Option<BTreeMap<Vec<u8>, Op>>,
    store: Rc<Mutex<Merk>>
}

// TODO: throw instead of panicking
// TODO: make this code succinct

macro_rules! buffer_arg_to_vec {
    ($cx:ident, $index:expr) => {
        {
            let buffer = $cx.argument::<JsBuffer>($index)?;
            $cx.borrow(
                &buffer,
                |buffer| buffer.as_slice().to_vec()
            )
        }
    };
}

declare_types! {
    pub class JsMerk for MerkHandle {
        init(mut cx) {
            let path = cx.argument::<JsString>(0)?.value();
            let path = Path::new(&path);
            let store = Merk::open(path).unwrap();
            Ok(MerkHandle { store: Rc::new(Mutex::new(store)) })
        }

        method getSync(mut cx) {
            let key = buffer_arg_to_vec!(cx, 0);

            let value = {
                let this = cx.this();
                let guard = cx.lock();
                let handle = this.borrow(&guard);
                let store = handle.store.lock().unwrap();
                store.get(key.as_slice()).unwrap()
            };

            let buffer = cx.buffer(value.len() as u32)?;
            for i in 0..value.len() {
                let n = cx.number(value[i]);
                buffer.set(&mut cx, i as u32, n)?;
            }
            Ok(buffer.upcast())
        }

        method rootHash(mut cx) {
            let hash = {
                let this = cx.this();
                let guard = cx.lock();
                let handle = this.borrow(&guard);
                let store = handle.store.lock().unwrap();
                store.root_hash()
            };

            let buffer = cx.buffer(20)?;
            for i in 0..20 {
                let n = cx.number(hash[i]);
                buffer.set(&mut cx, i as u32, n)?;
            }
            Ok(buffer.upcast())
        }

        method batch(mut cx) {
            let args: Vec<Handle<JsMerk>> = vec![ cx.this() ];
            Ok(JsBatch::new(&mut cx, args)?.upcast())
        }

        method flushSync(mut cx) {
            {
                let this = cx.this();
                let guard = cx.lock();
                let handle = this.borrow(&guard);
                let store = handle.store.lock().unwrap();
                store.flush().unwrap();
            }

            Ok(cx.undefined().upcast())
        }
    }

    pub class JsBatch for Batch {
        init(mut cx) {
            let merk = cx.argument::<JsMerk>(0)?;
            let guard = cx.lock();
            let handle = merk.borrow(&guard);
            Ok(Batch {
                ops: Some(BTreeMap::new()),
                store: handle.store.clone()
            })
        }

        method put(mut cx) {
            let res = {
                let key = buffer_arg_to_vec!(cx, 0);
                let value = buffer_arg_to_vec!(cx, 1);
                // TODO: assert lengths

                let mut this = cx.this();
                let guard = cx.lock();
                let mut handle = this.borrow_mut(&guard);

                if let Some(ops) = &mut handle.ops {
                    ops.insert(key, Op::Put(value));
                    Ok(())
                } else {
                    Err("batch was already committed")
                }
            };

            match res {
                Ok(_) => Ok(cx.this().upcast()),
                Err(err) => cx.throw_error(err)
            }
        }

        method delete(mut cx) {
            let res = {
                let key = buffer_arg_to_vec!(cx, 0);
                // TODO: assert length

                let mut this = cx.this();
                let guard = cx.lock();
                let mut handle = this.borrow_mut(&guard);

                if let Some(ops) = &mut handle.ops {
                    ops.insert(key, Op::Delete);
                    Ok(())
                } else {
                    Err("batch was already committed")
                }
            };

            match res {
                Ok(_) => Ok(cx.this().upcast()),
                Err(err) => cx.throw_error(err)
            }
        }

        method commitSync(mut cx) {
            let mut do_commit = || {
                let mut this = cx.this();
                let guard = cx.lock();
                let mut handle = this.borrow_mut(&guard);

                if let Some(ops) = handle.ops.take() {
                    let mut batch = Vec::with_capacity(ops.len());
                    for entry in ops {
                        batch.push(entry);
                    }
                    let mut store = handle.store.lock().unwrap();
                    store.apply(batch.as_slice()).unwrap();
                    Ok(())
                } else {
                    Err("batch was already committed")
                }
            };

            match do_commit() {
                Ok(_) => Ok(cx.undefined().upcast()),
                Err(err) => cx.throw_error(err)
            }
        }

        method commit(mut cx) {
            cx.throw_error("not yet implemented")
        }
    }
}

register_module!(mut m, {
    m.export_class::<JsMerk>("Merk")
});
