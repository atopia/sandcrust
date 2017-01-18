extern crate rustc_serialize;
extern crate sandcrust;
use sandcrust::*;

#[cfg(test)]
mod complex_restore {
    use super::*;

    #[derive(RustcEncodable, RustcDecodable, PartialEq)]
    struct Entity {
        x: f32,
        y: f32,
    }

    #[derive(RustcEncodable, RustcDecodable, PartialEq)]
    struct World {
        entities: Vec<Entity>
    }

    fn complex_struct_vec(world: &mut World) {
        world.entities[0] = Entity{ x: 1.0, .. world.entities[0]};
    }

    #[test]
    fn complex_struct_vec_test() {
        let mut world = World {
            entities: vec![Entity {x: 0.0, y: 4.0}, Entity {x: 10.0, y: 20.5}]
        };
        let new_world = World {
            entities: vec![Entity {x: 1.0, y: 4.0}, Entity {x: 10.0, y: 20.5}]
        };
        sandbox_no_ret!(complex_struct_vec(&mut world));
        assert!(world == new_world)
    }

    // make sure the comparison is a useful one
    #[test]
    #[should_panic(expected = "assertion failed")]
    fn complex_struct_vec_test_fail() {
        let world = World {
            entities: vec![Entity {x: 0.0, y: 4.0}, Entity {x: 10.0, y: 20.5}]
        };
        let new_world = World {
            entities: vec![Entity {x: 1.0, y: 4.0}, Entity {x: 10.0, y: 20.5}]
        };
        assert!(world == new_world)
    }
}
