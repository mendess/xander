use std::ops::{Deref, DerefMut};

use cursive::{event::Key, views::OnEventView, View};

pub struct VimView<V>(V);

pub trait ViewExt: Sized {
    fn with_vim_keys(self) -> VimView<Self>;

    fn esq_to_quit(self) -> OnEventView<Self>;
}

impl<V> ViewExt for V {
    fn with_vim_keys(self) -> VimView<Self> {
        VimView(self)
    }

    fn esq_to_quit(self) -> OnEventView<Self> {
        OnEventView::new(self).on_event(Key::Esc, |s| {
            s.pop_layer();
        })
    }
}

impl<V> Deref for VimView<V> {
    type Target = V;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V> DerefMut for VimView<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<V> View for VimView<V>
where
    V: View + 'static,
{
    fn draw(&self, printer: &cursive::Printer) {
        self.0.draw(printer);
    }

    fn layout(&mut self, xy: cursive::Vec2) {
        self.0.layout(xy)
    }

    fn needs_relayout(&self) -> bool {
        self.0.needs_relayout()
    }

    fn required_size(&mut self, constraint: cursive::Vec2) -> cursive::Vec2 {
        self.0.required_size(constraint)
    }

    fn on_event(&mut self, ev: cursive::event::Event) -> cursive::event::EventResult {
        V::on_event(&mut self.0, translate_vim_keys(ev))
    }

    fn call_on_any(&mut self, selector: &cursive::view::Selector, any_cb: cursive::event::AnyCb) {
        self.0.call_on_any(selector, any_cb)
    }

    fn focus_view(
        &mut self,
        sel: &cursive::view::Selector,
    ) -> Result<cursive::event::EventResult, cursive::view::ViewNotFound> {
        self.0.focus_view(sel)
    }

    fn take_focus(
        &mut self,
        source: cursive::direction::Direction,
    ) -> Result<cursive::event::EventResult, cursive::view::CannotFocus> {
        self.0.take_focus(source)
    }

    fn important_area(&self, view_size: cursive::Vec2) -> cursive::Rect {
        self.0.important_area(view_size)
    }
}

fn translate_vim_keys(ev: cursive::event::Event) -> cursive::event::Event {
    use cursive::event::Event;
    match ev {
        Event::Char('j') => Event::Key(Key::Down),
        Event::Char('k') => Event::Key(Key::Up),
        Event::Char('h') => Event::Key(Key::Left),
        Event::Char('l') => Event::Key(Key::Right),
        ev => ev,
    }
}
