mod without_registry {
    use flatpg::{
        graph::{Graph, builder::GraphDiff},
        prelude::*,
        schema::{Schema, Version},
    };

    #[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, PropertyItemKind)]
    enum MacroProperty {
        #[property(typ = String, quantity = One)]
        Key,
    }

    #[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, NodeItemKind)]
    #[node_kind(schema = MacroSchema, property_kind = MacroProperty)]
    enum MacroNode {
        #[properties(Key)]
        A,
    }

    #[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, EdgeItemKind)]
    #[edge_kind(schema = MacroSchema)]
    enum MacroEdge {
        #[property(typ = None)]
        Base,
    }

    schema!(
        name = MacroSchema,
        node_kind = MacroNode,
        edge_kind = MacroEdge,
        prop_kind = MacroProperty,
        version = "1.0.0"
    );

    #[test]
    fn schema_macro_defaults_epr_to_no_enum_props() {
        let mut diff = GraphDiff::<MacroSchema>::default();
        diff.add_node(
            builders::ANodeBuilder::new()
                .add_property(MacroProperty::Key, "hello".to_string())
                .unwrap()
                .build(),
        );

        let (graph, _) = diff.apply(Graph::<MacroSchema>::new()).expect("apply diff");

        let Some(Node::A(a)) = graph.a().next() else {
            panic!("expected Node::A");
        };
        assert_eq!(a.key().unwrap(), "hello");
    }

    #[test]
    fn schema_macro_sets_name_and_version_consts() {
        assert_eq!(MacroSchema::NAME, "MacroSchema");
        assert_eq!(MacroSchema::VERSION, Version::new(1, 0, 0));
    }

    #[test]
    fn schema_accessors_return_the_consts() {
        assert_eq!(MacroSchema::name(), MacroSchema::NAME);
        assert_eq!(MacroSchema::version(), MacroSchema::VERSION);
    }
}

mod with_registry {
    use flatpg::{
        graph::{Graph, builder::GraphDiff},
        prelude::*,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumProperty)]
    enum MacroStatus {
        On,
        Off,
    }

    enum_property_registry!(MacroRegistry: MacroStatus);

    #[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, PropertyItemKind)]
    enum MacroProperty {
        #[property(typ = Enum<MacroStatus>, quantity = One)]
        State,
    }

    #[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, NodeItemKind)]
    #[node_kind(schema = MacroSchema, property_kind = MacroProperty)]
    enum MacroNode {
        #[properties(State)]
        A,
    }

    #[derive(Clone, Copy, Hash, PartialOrd, Ord, PartialEq, Eq, Debug, EdgeItemKind)]
    #[edge_kind(schema = MacroSchema)]
    enum MacroEdge {
        #[property(typ = None)]
        Base,
    }

    schema!(
        name = MacroSchema,
        node_kind = MacroNode,
        edge_kind = MacroEdge,
        prop_kind = MacroProperty,
        enum_prop_registry = MacroRegistry,
        version = "1.0.0"
    );

    #[test]
    fn schema_macro_accepts_explicit_enum_property_registry() {
        let mut diff = GraphDiff::<MacroSchema>::default();
        diff.add_node(
            builders::ANodeBuilder::new()
                .add_property(MacroProperty::State, MacroStatus::On)
                .unwrap()
                .build(),
        );
        let (graph, _) = diff.apply(Graph::<MacroSchema>::new()).expect("apply diff");

        let Some(Node::A(a)) = graph.a().next() else {
            panic!("expected Node::A");
        };
        assert_eq!(a.state().unwrap(), MacroStatus::On);
    }
}
