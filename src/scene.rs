//! Displays a [`Tabs`] widget to select the content to be displayed.
//!
//! This is a wrapper around the [`TabBar`] widget.
//! Unlike the [`TabBar`] widget it will also handle
//! the content of the tabs.
//!
//! *This API requires the following crate features to be activated: tabs*

use iced::{
	Element, Event, Length, Point, Rectangle, Size, Vector,
	advanced::{
		Clipboard, Layout, Shell, Widget,
		layout::{Limits, Node},
		overlay, renderer,
		widget::{
			Operation, Tree,
			tree::{State, Tag},
		},
	},
	mouse::Cursor,
	widget::{self, Row},
};

/// A [`Tabs`] widget for showing a [`TabBar`]
/// along with the tab's content.
///
/// # Example
/// ```ignore
/// # use iced_aw::{TabLabel, tabs::Tabs};
/// # use iced_widget::Text;
/// #
/// #[derive(Debug, Clone)]
/// enum Message {
///     TabSelected(TabId),
/// }
///
/// #[derive(Debug, Clone)]
/// enum TabId {
///    One,
///    Two,
///    Three,
/// }
///
/// let tabs = Tabs::new(Message::TabSelected)
/// .push(TabId::One, TabLabel::Text(String::from("One")), Text::new(String::from("One")))
/// .push(TabId::Two, TabLabel::Text(String::from("Two")), Text::new(String::from("Two")))
/// .push(TabId::Three, TabLabel::Text(String::from("Three")), Text::new(String::from("Three")))
/// .set_active_tab(&TabId::Two);Theme
/// ```
///
#[allow(missing_debug_implementations)]
pub struct Scene<'a, Message, SceneId, Theme = widget::Theme, Renderer = widget::Renderer>
where
	Renderer: 'a + renderer::Renderer,
	SceneId: Eq + Clone,
{
	active_scene_index: usize,
	/// The vector containing the content of the tabs.
	children: Vec<Element<'a, Message, Theme, Renderer>>,
	/// The vector containing the indices of the tabs.
	indices: Vec<SceneId>,
}

impl<'a, Message, SceneId, Theme, Renderer> Scene<'a, Message, SceneId, Theme, Renderer>
where
	Renderer: 'a + renderer::Renderer,
	SceneId: Eq + Clone,
{
	/// Similar to `new` but with a given Vector of the
	/// [`TabLabel`] along with the tab's content.
	///
	/// It expects:
	///     * the index of the currently active tab.
	///     * a vector containing the [`TabLabel`]s along with the content
	///         [`Element`]s of the [`Tabs`].
	///     * the function that will be called if a tab is selected by the user.
	///         It takes the index of the selected tab.
	pub fn new(
		tabs: impl IntoIterator<Item = (SceneId, Element<'a, Message, Theme, Renderer>)>,
	) -> Self {
		let tabs = tabs.into_iter();
		let n_tabs = tabs.size_hint().0;

		let mut elements = Vec::with_capacity(n_tabs);
		let mut indices = Vec::with_capacity(n_tabs);

		for (id, element) in tabs {
			indices.push(id);
			elements.push(element);
		}
		assert!(!indices.is_empty());

		Scene {
			active_scene_index: 0,
			children: elements,
			indices,
		}
	}

	pub fn set_active_scene(mut self, id: SceneId) -> Self {
		self.active_scene_index = self.indices.iter().position(|i| *i == id).unwrap_or(0);
		self
	}
}

impl<Message, SceneId, Theme, Renderer> Widget<Message, Theme, Renderer>
	for Scene<'_, Message, SceneId, Theme, Renderer>
where
	Renderer: renderer::Renderer,
	SceneId: Eq + Clone,
{
	fn children(&self) -> Vec<Tree> {
		let tabs = Tree {
			tag: Tag::stateless(),
			state: State::None,
			children: self.children.iter().map(Tree::new).collect(),
		};

		vec![tabs]
	}

	fn diff(&self, tree: &mut Tree) {
		if tree.children.len() != 1 {
			tree.children = self.children();
		}

		if let Some(tabs) = tree.children.first_mut() {
			tabs.diff_children(&self.children);
		}
	}

	fn size(&self) -> Size<Length> {
		Size::new(Length::Fill, Length::Fill)
	}

	fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
		let tab_content_limits = limits.width(Length::Fill).height(Length::Fill);

		let mut tab_content_node = if let (Some(element), Some(child)) = (
			self.children.get_mut(self.active_scene_index),
			tree.children.first_mut(),
		) {
			element.as_widget_mut().layout(
				&mut child.children[self.active_scene_index],
				renderer,
				&tab_content_limits,
			)
		} else {
			Row::<Message, Theme, Renderer>::new()
				.width(Length::Fill)
				.height(Length::Fill)
				.layout(tree, renderer, &tab_content_limits)
		};

		let tab_content_bounds = tab_content_node.bounds();
		tab_content_node = tab_content_node
			.move_to(Point::new(tab_content_bounds.x, tab_content_bounds.y));

		Node::with_children(
			Size::new(
				tab_content_node.size().width,
				tab_content_node.size().height,
			),
			vec![tab_content_node],
		)
	}

	fn update(
		&mut self,
		state: &mut Tree,
		event: &Event,
		layout: Layout<'_>,
		cursor: Cursor,
		renderer: &Renderer,
		clipboard: &mut dyn Clipboard,
		shell: &mut Shell<'_, Message>,
		viewport: &Rectangle,
	) {
		let mut children = layout.children();
		let tab_content_layout = children
			.next()
			.expect("widget: Layout should have a content at top position");

		let idx = self.active_scene_index;
		if let Some(element) = self.children.get_mut(idx) {
			element.as_widget_mut().update(
				&mut state.children[0].children[idx],
				event,
				tab_content_layout,
				cursor,
				renderer,
				clipboard,
				shell,
				viewport,
			);
		}
	}

	fn draw(
		&self,
		state: &Tree,
		renderer: &mut Renderer,
		theme: &Theme,
		style: &renderer::Style,
		layout: Layout<'_>,
		cursor: Cursor,
		viewport: &Rectangle,
	) {
		let mut children = layout.children();
		let tab_content_layout = children
			.next()
			.expect("Graphics: There should be a TabBar at the bottom position");

		let idx = self.active_scene_index;
		if let Some(element) = self.children.get(idx) {
			element.as_widget().draw(
				&state.children[0].children[idx],
				renderer,
				theme,
				style,
				tab_content_layout,
				cursor,
				viewport,
			);
		}
	}

	fn overlay<'b>(
		&'b mut self,
		state: &'b mut Tree,
		layout: Layout<'b>,
		renderer: &Renderer,
		viewport: &Rectangle,
		translation: Vector,
	) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
		let layout = layout.children().next();

		layout.and_then(|layout| {
			let idx = self.active_scene_index;
			self.children
				.get_mut(idx)
				.map(Element::as_widget_mut)
				.and_then(|w| {
					w.overlay(
						&mut state.children[0].children[idx],
						layout,
						renderer,
						viewport,
						translation,
					)
				})
		})
	}

	fn operate(
		&mut self,
		tree: &mut Tree,
		layout: Layout<'_>,
		renderer: &Renderer,
		operation: &mut dyn Operation<()>,
	) {
		let active_tab = self.active_scene_index;
		operation.container(None, layout.bounds());
		operation.traverse(&mut |operation| {
			self.children[active_tab].as_widget_mut().operate(
				&mut tree.children[0].children[active_tab],
				layout.children().next().expect("No contents"),
				renderer,
				operation,
			);
		});
	}
}

impl<'a, Message, TabId, Theme, Renderer> From<Scene<'a, Message, TabId, Theme, Renderer>>
	for Element<'a, Message, Theme, Renderer>
where
	Renderer: 'a + renderer::Renderer,
	Theme: 'a,
	Message: 'a,
	TabId: 'a + Eq + Clone,
{
	fn from(scene: Scene<'a, Message, TabId, Theme, Renderer>) -> Self {
		Element::new(scene)
	}
}
