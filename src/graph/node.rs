//! Node API.
//!
//! ⚠️ This API is under construction and is missing some features. ⚠️
//!
//! The Node API is designed for effect editing, and the creation of UI tools.
//! It builds on top of the [Expression API] and is entirely optional. It
//! defines a [`Graph`] composed of [`Node`]s. Each node represent either a
//! [`Modifier`] or an [`Expr`]. A node has some [`Slot`]s associated with it,
//! representing its inputs and outputs, if any. An _output_ slot of a node can
//! be linked to an _input_ slot of another node to express that the value
//! produced by the upstream node must flow through to the input of the
//! downstream node.
//!
//! An effect [`Graph`] can be serialized as is, to retain its editing
//! capabilities. Alternatively, once the user has finished building an effect,
//! it can be converted to a runtime [`EffectAsset`] for use as a
//! [`ParticleEffect`].
//!
//! [Expression API]: crate::graph::expr
//! [`Modifier`]: crate::Modifier
//! [`Expr`]: crate::graph::expr::Expr
//! [`EffectAsset`]: crate::EffectAsset
//! [`ParticleEffect`]: crate::ParticleEffect

use std::num::NonZeroU32;

use crate::{Attribute, BuiltInOperator, ExprError, ExprHandle, Module, ValueType};

/// Identifier of a node in a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(NonZeroU32);

impl NodeId {
    /// Create a new node identifier.
    pub fn new(id: NonZeroU32) -> Self {
        Self(id)
    }

    /// Get the one-based node index.
    pub fn id(&self) -> NonZeroU32 {
        self.0
    }

    /// Get the zero-based index of the node in the underlying graph node array.
    pub fn index(&self) -> usize {
        (self.0.get() - 1) as usize
    }
}

/// Identifier of a slot in a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotId(NonZeroU32);

impl SlotId {
    /// Create a new slot identifier.
    pub fn new(id: NonZeroU32) -> Self {
        Self(id)
    }

    /// Get the one-based slot index.
    pub fn id(&self) -> NonZeroU32 {
        self.0
    }

    /// Get the zero-based index of the slot in the underlying graph slot array.
    pub fn index(&self) -> usize {
        (self.0.get() - 1) as usize
    }
}

/// Node slot direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotDir {
    /// Input slot receiving data from outside the node.
    Input,
    /// Output slot providing data generated by the node.
    Output,
}

/// Definition of a slot of a node.
#[derive(Debug, Clone)]
pub struct SlotDef {
    /// Slot name.
    name: String,
    /// Slot direaction.
    dir: SlotDir,
    /// Type of values accepted by the slot. This may be `None` for variant
    /// slots, if the type depends on the inputs of the node during evaluation.
    value_type: Option<ValueType>,
}

impl SlotDef {
    /// Create a new input slot.
    pub fn input(name: impl Into<String>, value_type: Option<ValueType>) -> Self {
        Self {
            name: name.into(),
            dir: SlotDir::Input,
            value_type,
        }
    }

    /// Create a new output slot.
    pub fn output(name: impl Into<String>, value_type: Option<ValueType>) -> Self {
        Self {
            name: name.into(),
            dir: SlotDir::Output,
            value_type,
        }
    }

    /// Get the slot name.
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the slot direction.
    #[inline]
    pub fn dir(&self) -> SlotDir {
        self.dir
    }

    /// Is the slot an input slot?
    #[inline]
    pub fn is_input(&self) -> bool {
        self.dir == SlotDir::Input
    }

    /// Is the slot an input slot?
    #[inline]
    pub fn is_output(&self) -> bool {
        self.dir == SlotDir::Output
    }

    /// Get the slot value type.
    #[inline]
    pub fn value_type(&self) -> Option<ValueType> {
        self.value_type
    }
}

/// Single slot of a node.
#[derive(Debug, Clone)]
pub struct Slot {
    /// Owner node identifier.
    node_id: NodeId,
    /// Identifier.
    id: SlotId,
    /// Slot definition.
    def: SlotDef,
    /// Linked slots.
    linked_slots: Vec<SlotId>,
}

impl Slot {
    /// Create a new slot.
    pub fn new(node_id: NodeId, slot_id: SlotId, slot_def: SlotDef) -> Self {
        Slot {
            node_id,
            id: slot_id,
            def: slot_def,
            linked_slots: vec![],
        }
    }

    /// Get the node identifier of the node this slot is from.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Get the slot identifier.
    pub fn id(&self) -> SlotId {
        self.id
    }

    /// Get the slot definition.
    pub fn def(&self) -> &SlotDef {
        &self.def
    }

    /// Get the slot direction.
    pub fn dir(&self) -> SlotDir {
        self.def.dir()
    }

    /// Check if this slot is an input slot.
    ///
    /// This is a convenience helper for `self.dir() == SlotDir::Input`.
    pub fn is_input(&self) -> bool {
        self.dir() == SlotDir::Input
    }

    /// Check if this slot is an output slot.
    ///
    /// This is a convenience helper for `self.dir() == SlotDir::Output`.
    pub fn is_output(&self) -> bool {
        self.dir() == SlotDir::Output
    }

    /// Link this output slot to an input slot.
    ///
    /// # Panics
    ///
    /// Panics if this slot's direction is `SlotDir::Input`.
    fn link_to(&mut self, input: SlotId) {
        assert!(self.is_output());
        if !self.linked_slots.contains(&input) {
            self.linked_slots.push(input);
        }
    }

    fn unlink_from(&mut self, input: SlotId) -> bool {
        assert!(self.is_output());
        if let Some(index) = self.linked_slots.iter().position(|&s| s == input) {
            self.linked_slots.remove(index);
            true
        } else {
            false
        }
    }

    fn link_input(&mut self, output: SlotId) {
        assert!(self.is_input());
        if self.linked_slots.is_empty() {
            self.linked_slots.push(output);
        } else {
            self.linked_slots[0] = output;
        }
    }

    fn unlink_input(&mut self) {
        assert!(self.is_input());
        self.linked_slots.clear();
    }
}

/// Effect graph.
///
/// An effect graph represents an editable version of an [`EffectAsset`]. The
/// graph is composed of [`Node`]s, which represent either a [`Modifier`] or an
/// expression [`Expr`]. Expression nodes are linked together to form more
/// complex expressions which are then assigned to the modifier inputs. Once the
/// graph is ready, it can be converted into an [`EffectAsset`].
///
/// [`EffectAsset`]: crate::EffectAsset
/// [`Modifier`]: crate::Modifier
/// [`Expr`]: crate::graph::Expr
#[derive(Default)]
pub struct Graph {
    nodes: Vec<Box<dyn Node>>,
    slots: Vec<Slot>,
}

impl std::fmt::Debug for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph").field("slots", &self.slots).finish()
    }
}

impl Graph {
    /// Create a new empty graph.
    ///
    /// An empty graph doesn't represent a valid [`EffectAsset`]. You must add
    /// some [`Node`]s with [`add_node()`] and [`link()`] them together to form
    /// a valid graph.
    ///
    /// [`EffectAsset`]: crate::EffectAsset
    /// [`add_node()`]: crate::graph::Graph::add_node
    /// [`link()`]: crate::graph::Graph::link
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node to the graph, without any link.
    ///
    /// # Example
    ///
    /// ```
    /// # use bevy_hanabi::*;
    /// let mut graph = Graph::new();
    /// let time_node = graph.add_node(TimeNode::default());
    /// ```
    #[inline]
    pub fn add_node<N>(&mut self, node: N) -> NodeId
    where
        N: Node + 'static,
    {
        self.add_node_impl(Box::new(node))
    }

    fn add_node_impl(&mut self, node: Box<dyn Node>) -> NodeId {
        let index = self.nodes.len() as u32;
        let node_id = NodeId::new(NonZeroU32::new(index + 1).unwrap());

        for slot_def in node.slots() {
            let slot_id = SlotId::new(NonZeroU32::new(self.slots.len() as u32 + 1).unwrap());
            let slot = Slot::new(node_id, slot_id, slot_def.clone());
            self.slots.push(slot);
        }

        self.nodes.push(node);

        node_id
    }

    /// Link an output slot of a node to an input slot of another node.
    ///
    /// # Panics
    ///
    /// Panics if the `output` argument doesn't reference an output slot of an
    /// existing node, or the `input` argument doesn't reference an input slot
    /// of an existing node.
    pub fn link(&mut self, output: SlotId, input: SlotId) {
        let out_slot = self.get_slot_mut(output);
        assert!(out_slot.is_output());
        out_slot.link_to(input);

        let in_slot = self.get_slot_mut(input);
        assert!(in_slot.is_input());
        in_slot.link_input(output);
    }

    /// Unlink an output slot of a node from an input slot of another node.
    ///
    /// # Panics
    ///
    /// Panics if the `output` argument doesn't reference an output slot of an
    /// existing node, or the `input` argument doesn't reference an input slot
    /// of an existing node.
    pub fn unlink(&mut self, output: SlotId, input: SlotId) {
        let out_slot = self.get_slot_mut(output);
        assert!(out_slot.is_output());
        if out_slot.unlink_from(input) {
            let in_slot = self.get_slot_mut(input);
            assert!(in_slot.is_input());
            in_slot.unlink_input();
        }
    }

    /// Unlink all remote slots from a given slot.
    pub fn unlink_all(&mut self, slot_id: SlotId) {
        let slot = self.get_slot_mut(slot_id);
        let linked_slots = std::mem::take(&mut slot.linked_slots);
        for remote_id in &linked_slots {
            let remote_slot = self.get_slot_mut(*remote_id);
            if remote_slot.is_input() {
                remote_slot.unlink_input();
            } else {
                remote_slot.unlink_from(slot_id);
            }
        }
    }

    /// Get all slots of a node.
    pub fn slots(&self, node_id: NodeId) -> Vec<SlotId> {
        self.slots
            .iter()
            .filter_map(|s| {
                if s.node_id() == node_id {
                    Some(s.id())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get a given input slot of a node by name.
    pub fn input_slot<'a, 'b: 'a, S: Into<&'b str>>(
        &'a self,
        node_id: NodeId,
        name: S,
    ) -> Option<SlotId> {
        let name = name.into();
        self.slots
            .iter()
            .find(|s| s.node_id() == node_id && s.is_input() && s.def().name() == name)
            .map(|s| s.id)
    }

    /// Get all input slots of a node.
    pub fn input_slots(&self, node_id: NodeId) -> Vec<SlotId> {
        self.slots
            .iter()
            .filter_map(|s| {
                if s.node_id() == node_id && s.is_input() {
                    Some(s.id())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get a given output slot of a node by name.
    pub fn output_slot<'a, 'b: 'a, S: Into<&'b str>>(
        &'a self,
        node_id: NodeId,
        name: S,
    ) -> Option<SlotId> {
        let name = name.into();
        self.slots
            .iter()
            .find(|s| s.node_id() == node_id && s.is_output() && s.def().name() == name)
            .map(|s| s.id)
    }

    /// Get all output slots of a node.
    pub fn output_slots(&self, node_id: NodeId) -> Vec<SlotId> {
        self.slots
            .iter()
            .filter_map(|s| {
                if s.node_id() == node_id && s.is_output() {
                    Some(s.id())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Find a slot ID by slot name.
    pub fn get_slot_id<'a, 'b: 'a, S: Into<&'b str>>(&'a self, name: S) -> Option<SlotId> {
        let name = name.into();
        self.slots
            .iter()
            .find(|&s| s.def().name() == name)
            .map(|s| s.id)
    }

    #[allow(dead_code)] // TEMP
    fn get_slot(&self, id: SlotId) -> &Slot {
        let index = id.index();
        assert!(index < self.slots.len());
        &self.slots[index]
    }

    fn get_slot_mut(&mut self, id: SlotId) -> &mut Slot {
        let index = id.index();
        assert!(index < self.slots.len());
        &mut self.slots[index]
    }
}

/// Generic graph node.
pub trait Node {
    /// Get the list of slots of this node.
    ///
    /// The list contains both input and output slots, without any guaranteed
    /// order.
    fn slots(&self) -> &[SlotDef];

    /// Evaluate the node from the given input expressions, and optionally
    /// produce output expression(s).
    ///
    /// The expressions themselves are not evaluated (that is, _e.g._ "3 + 2" is
    /// _not_ reduced to "5").
    fn eval(
        &self,
        module: &mut Module,
        inputs: Vec<ExprHandle>,
    ) -> Result<Vec<ExprHandle>, ExprError>;
}

/// Graph node to add two values.
#[derive(Debug, Clone)]
pub struct AddNode {
    slots: [SlotDef; 3],
}

impl Default for AddNode {
    fn default() -> Self {
        Self {
            slots: [
                SlotDef::input("lhs", None),
                SlotDef::input("rhs", None),
                SlotDef::output("result", None),
            ],
        }
    }
}

impl Node for AddNode {
    fn slots(&self) -> &[SlotDef] {
        &self.slots
    }

    fn eval(
        &self,
        module: &mut Module,
        inputs: Vec<ExprHandle>,
    ) -> Result<Vec<ExprHandle>, ExprError> {
        if inputs.len() != 2 {
            return Err(ExprError::GraphEvalError(format!(
                "Unexpected input count to AddNode::eval(): expected 2, got {}",
                inputs.len()
            )));
        }
        let mut inputs = inputs.into_iter();
        let left = inputs.next().unwrap();
        let right = inputs.next().unwrap();
        let add = module.add(left, right);
        Ok(vec![add])
    }
}

/// Graph node to subtract two values.
#[derive(Debug, Clone)]
pub struct SubNode {
    slots: [SlotDef; 3],
}

impl Default for SubNode {
    fn default() -> Self {
        Self {
            slots: [
                SlotDef::input("lhs", None),
                SlotDef::input("rhs", None),
                SlotDef::output("result", None),
            ],
        }
    }
}

impl Node for SubNode {
    fn slots(&self) -> &[SlotDef] {
        &self.slots
    }

    fn eval(
        &self,
        module: &mut Module,
        inputs: Vec<ExprHandle>,
    ) -> Result<Vec<ExprHandle>, ExprError> {
        if inputs.len() != 2 {
            return Err(ExprError::GraphEvalError(format!(
                "Unexpected input count to SubNode::eval(): expected 2, got
{}",
                inputs.len()
            )));
        }
        let mut inputs = inputs.into_iter();
        let left = inputs.next().unwrap();
        let right = inputs.next().unwrap();
        let sub = module.sub(left, right);
        Ok(vec![sub])
    }
}

/// Graph node to multiply two values.
#[derive(Debug, Clone)]
pub struct MulNode {
    slots: [SlotDef; 3],
}

impl Default for MulNode {
    fn default() -> Self {
        Self {
            slots: [
                SlotDef::input("lhs", None),
                SlotDef::input("rhs", None),
                SlotDef::output("result", None),
            ],
        }
    }
}

impl Node for MulNode {
    fn slots(&self) -> &[SlotDef] {
        &self.slots
    }

    fn eval(
        &self,
        module: &mut Module,
        inputs: Vec<ExprHandle>,
    ) -> Result<Vec<ExprHandle>, ExprError> {
        if inputs.len() != 2 {
            return Err(ExprError::GraphEvalError(format!(
                "Unexpected input count to MulNode::eval(): expected 2, got
{}",
                inputs.len()
            )));
        }
        let mut inputs = inputs.into_iter();
        let left = inputs.next().unwrap();
        let right = inputs.next().unwrap();
        let mul = module.mul(left, right);
        Ok(vec![mul])
    }
}

/// Graph node to divide two values.
#[derive(Debug, Clone)]
pub struct DivNode {
    slots: [SlotDef; 3],
}

impl Default for DivNode {
    fn default() -> Self {
        Self {
            slots: [
                SlotDef::input("lhs", None),
                SlotDef::input("rhs", None),
                SlotDef::output("result", None),
            ],
        }
    }
}

impl Node for DivNode {
    fn slots(&self) -> &[SlotDef] {
        &self.slots
    }

    fn eval(
        &self,
        module: &mut Module,
        inputs: Vec<ExprHandle>,
    ) -> Result<Vec<ExprHandle>, ExprError> {
        if inputs.len() != 2 {
            return Err(ExprError::GraphEvalError(format!(
                "Unexpected input count to DivNode::eval(): expected 2, got
{}",
                inputs.len()
            )));
        }
        let mut inputs = inputs.into_iter();
        let left = inputs.next().unwrap();
        let right = inputs.next().unwrap();
        let div = module.div(left, right);
        Ok(vec![div])
    }
}

/// Graph node to get any single particle attribute.
#[derive(Debug, Clone)]
pub struct AttributeNode {
    /// The attribute to get.
    attr: Attribute,
    /// The output slot corresponding to the get value.
    slots: [SlotDef; 1],
}

impl Default for AttributeNode {
    fn default() -> Self {
        Self::new(Attribute::POSITION)
    }
}

impl AttributeNode {
    /// Create a new attribute node for the given [`Attribute`].
    pub fn new(attr: Attribute) -> Self {
        Self {
            attr,
            slots: [SlotDef::output(attr.name(), Some(attr.value_type()))],
        }
    }
}

impl AttributeNode {
    /// Get the attribute this node reads.
    pub fn attr(&self) -> Attribute {
        self.attr
    }

    /// Set the attribute this node reads.
    pub fn set_attr(&mut self, attr: Attribute) {
        self.attr = attr;
    }
}

impl Node for AttributeNode {
    fn slots(&self) -> &[SlotDef] {
        &self.slots
    }

    fn eval(
        &self,
        module: &mut Module,
        inputs: Vec<ExprHandle>,
    ) -> Result<Vec<ExprHandle>, ExprError> {
        if !inputs.is_empty() {
            return Err(ExprError::GraphEvalError(
                "Unexpected non-empty input to
AttributeNode::eval()."
                    .to_string(),
            ));
        }
        let attr = module.attr(self.attr);
        Ok(vec![attr])
    }
}

/// Graph node to get various time values related to the effect system.
#[derive(Debug, Clone)]
pub struct TimeNode {
    /// Output slots corresponding to the various time-related quantities.
    slots: [SlotDef; 2],
}

impl Default for TimeNode {
    fn default() -> Self {
        Self {
            slots: [BuiltInOperator::Time, BuiltInOperator::DeltaTime]
                .map(|op| SlotDef::output(op.name(), Some(op.value_type()))),
        }
    }
}

impl Node for TimeNode {
    fn slots(&self) -> &[SlotDef] {
        &self.slots
    }

    fn eval(
        &self,
        module: &mut Module,
        inputs: Vec<ExprHandle>,
    ) -> Result<Vec<ExprHandle>, ExprError> {
        if !inputs.is_empty() {
            return Err(ExprError::GraphEvalError(
                "Unexpected non-empty input to
TimeNode::eval()."
                    .to_string(),
            ));
        }
        Ok([BuiltInOperator::Time, BuiltInOperator::DeltaTime]
            .map(|op| module.builtin(op))
            .to_vec())
    }
}

/// Graph node to normalize a vector value.
#[derive(Debug, Clone)]
pub struct NormalizeNode {
    /// Input and output vectors.
    slots: [SlotDef; 2],
}

impl Default for NormalizeNode {
    fn default() -> Self {
        Self {
            slots: [SlotDef::output("in", None), SlotDef::output("out", None)],
        }
    }
}

impl Node for NormalizeNode {
    fn slots(&self) -> &[SlotDef] {
        &self.slots
    }

    fn eval(
        &self,
        module: &mut Module,
        inputs: Vec<ExprHandle>,
    ) -> Result<Vec<ExprHandle>, ExprError> {
        if inputs.len() != 1 {
            return Err(ExprError::GraphEvalError(
                "Unexpected input slot count to NormalizeNode::eval() not
equal to one."
                    .to_string(),
            ));
        }
        let input = inputs.into_iter().next().unwrap();
        let norm = module.normalize(input);
        Ok(vec![norm])
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;
    use crate::{EvalContext, ModifierContext, ParticleLayout, PropertyLayout, ShaderWriter};

    #[test]
    fn add() {
        let node = AddNode::default();

        let mut module = Module::default();

        let ret = node.eval(&mut module, vec![]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));
        let three = module.lit(3.);
        let ret = node.eval(&mut module, vec![three]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));

        let two = module.lit(2.);
        let outputs = node.eval(&mut module, vec![three, two]).unwrap();
        assert_eq!(outputs.len(), 1);
        let out = outputs[0];

        let property_layout = PropertyLayout::default();
        let particle_layout = ParticleLayout::default();
        let mut context =
            ShaderWriter::new(ModifierContext::Update, &property_layout, &particle_layout);
        let str = context.eval(&module, out).unwrap();
        assert_eq!(str, "(3.) + (2.)".to_string());
    }

    #[test]
    fn sub() {
        let node = SubNode::default();

        let mut module = Module::default();

        let ret = node.eval(&mut module, vec![]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));
        let three = module.lit(3.);
        let ret = node.eval(&mut module, vec![three]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));

        let two = module.lit(2.);
        let outputs = node.eval(&mut module, vec![three, two]).unwrap();
        assert_eq!(outputs.len(), 1);
        let out = outputs[0];
        let property_layout = PropertyLayout::default();
        let particle_layout = ParticleLayout::default();
        let mut context =
            ShaderWriter::new(ModifierContext::Update, &property_layout, &particle_layout);
        let str = context.eval(&module, out).unwrap();
        assert_eq!(str, "(3.) - (2.)".to_string());
    }

    #[test]
    fn mul() {
        let node = MulNode::default();

        let mut module = Module::default();

        let ret = node.eval(&mut module, vec![]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));
        let three = module.lit(3.);
        let ret = node.eval(&mut module, vec![three]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));

        let two = module.lit(2.);
        let outputs = node.eval(&mut module, vec![three, two]).unwrap();
        assert_eq!(outputs.len(), 1);
        let out = outputs[0];
        let property_layout = PropertyLayout::default();
        let particle_layout = ParticleLayout::default();
        let mut context =
            ShaderWriter::new(ModifierContext::Update, &property_layout, &particle_layout);
        let str = context.eval(&module, out).unwrap();
        assert_eq!(str, "(3.) * (2.)".to_string());
    }

    #[test]
    fn div() {
        let node = DivNode::default();

        let mut module = Module::default();

        let ret = node.eval(&mut module, vec![]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));
        let three = module.lit(3.);
        let ret = node.eval(&mut module, vec![three]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));

        let two = module.lit(2.);
        let outputs = node.eval(&mut module, vec![three, two]).unwrap();
        assert_eq!(outputs.len(), 1);
        let out = outputs[0];
        let property_layout = PropertyLayout::default();
        let particle_layout = ParticleLayout::default();
        let mut context =
            ShaderWriter::new(ModifierContext::Update, &property_layout, &particle_layout);
        let str = context.eval(&module, out).unwrap();
        assert_eq!(str, "(3.) / (2.)".to_string());
    }

    #[test]
    fn attr() {
        let node = AttributeNode::new(Attribute::POSITION);

        let mut module = Module::default();

        let three = module.lit(3.);
        let ret = node.eval(&mut module, vec![three]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));

        let outputs = node.eval(&mut module, vec![]).unwrap();
        assert_eq!(outputs.len(), 1);
        let out = outputs[0];
        let property_layout = PropertyLayout::default();
        let particle_layout = ParticleLayout::default();
        let mut context =
            ShaderWriter::new(ModifierContext::Update, &property_layout, &particle_layout);
        let str = context.eval(&module, out).unwrap();
        assert_eq!(str, format!("particle.{}", Attribute::POSITION.name()));
    }

    #[test]
    fn time() {
        let node = TimeNode::default();

        let mut module = Module::default();

        let three = module.lit(3.);
        let ret = node.eval(&mut module, vec![three]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));

        let outputs = node.eval(&mut module, vec![]).unwrap();
        assert_eq!(outputs.len(), 2);
        let property_layout = PropertyLayout::default();
        let particle_layout = ParticleLayout::default();
        let mut context =
            ShaderWriter::new(ModifierContext::Update, &property_layout, &particle_layout);
        let str0 = context.eval(&module, outputs[0]).unwrap();
        let str1 = context.eval(&module, outputs[1]).unwrap();
        assert_eq!(str0, format!("sim_params.{}", BuiltInOperator::Time.name()));
        assert_eq!(
            str1,
            format!("sim_params.{}", BuiltInOperator::DeltaTime.name())
        );
    }

    #[test]
    fn normalize() {
        let node = NormalizeNode::default();

        let mut module = Module::default();

        let ret = node.eval(&mut module, vec![]);
        assert!(matches!(ret, Err(ExprError::GraphEvalError(_))));

        let ones = module.lit(Vec3::ONE);
        let outputs = node.eval(&mut module, vec![ones]).unwrap();
        assert_eq!(outputs.len(), 1);
        let property_layout = PropertyLayout::default();
        let particle_layout = ParticleLayout::default();
        let mut context =
            ShaderWriter::new(ModifierContext::Update, &property_layout, &particle_layout);
        let str = context.eval(&module, outputs[0]).unwrap();
        assert_eq!(str, "normalize(vec3<f32>(1.,1.,1.))".to_string());
    }

    #[test]
    fn graph() {
        let mut g = Graph::new();

        let nid_pos = g.add_node(AttributeNode::new(Attribute::POSITION));
        let nid_add = g.add_node(AddNode::default());
        let sid_pos = g.output_slots(nid_pos)[0];
        let sid_add_lhs = g.input_slots(nid_add)[0];
        let sid_add_rhs = g.input_slots(nid_add)[1];
        g.link(sid_pos, sid_add_lhs);

        let nid_vel = g.add_node(AttributeNode::new(Attribute::VELOCITY));
        let nid_mul = g.add_node(MulNode::default());
        let nid_dt = g.add_node(TimeNode::default());
        let sid_vel = g.output_slots(nid_vel)[0];
        let sid_dt = g
            .output_slot(nid_dt, BuiltInOperator::DeltaTime.name())
            .unwrap();
        let sid_mul_lhs = g.input_slots(nid_mul)[0];
        let sid_mul_rhs = g.input_slots(nid_mul)[1];
        g.link(sid_vel, sid_mul_lhs);
        g.link(sid_dt, sid_mul_rhs);

        let sid_mul_out = g.output_slots(nid_mul)[0];
        g.link(sid_mul_out, sid_add_rhs);
    }
}
