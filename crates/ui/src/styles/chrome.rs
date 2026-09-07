use gpui::{BoxShadow, Hsla, Pixels, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelChromeRadius {
    Control,
    Surface,
    Modal,
}

impl PixelChromeRadius {
    pub const fn pixels(self) -> Pixels {
        match self {
            Self::Control | Self::Surface => px(2.),
            Self::Modal => px(4.),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelChromeStroke {
    Border,
    FocusRing,
}

impl PixelChromeStroke {
    pub const fn width(self) -> Pixels {
        match self {
            Self::Border => px(1.),
            Self::FocusRing => px(2.),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelChromeShadow {
    Elevated,
    Modal,
}

impl PixelChromeShadow {
    pub const fn offset(self) -> Pixels {
        match self {
            Self::Elevated => px(2.),
            Self::Modal => px(4.),
        }
    }

    pub fn shadows(self, color: Hsla) -> Vec<BoxShadow> {
        let offset = self.offset();
        vec![BoxShadow::new(offset, offset, color)]
    }
}

#[cfg(test)]
mod tests {
    use gpui::{hsla, point, px};

    use super::*;

    #[test]
    fn pixel_chrome_geometry_matches_the_design_contract() {
        assert_eq!(PixelChromeRadius::Control.pixels(), px(2.));
        assert_eq!(PixelChromeRadius::Surface.pixels(), px(2.));
        assert_eq!(PixelChromeRadius::Modal.pixels(), px(4.));
        assert_eq!(PixelChromeStroke::Border.width(), px(1.));
        assert_eq!(PixelChromeStroke::FocusRing.width(), px(2.));
        assert_eq!(PixelChromeShadow::Elevated.offset(), px(2.));
        assert_eq!(PixelChromeShadow::Modal.offset(), px(4.));
    }

    #[test]
    fn pixel_chrome_shadows_are_single_hard_edges() {
        let color = hsla(0., 0., 0., 0.25);

        for (token, offset) in [
            (PixelChromeShadow::Elevated, px(2.)),
            (PixelChromeShadow::Modal, px(4.)),
        ] {
            let shadows = token.shadows(color);
            assert_eq!(shadows.len(), 1);
            let shadow = &shadows[0];
            assert_eq!(shadow.color, color);
            assert_eq!(shadow.offset, point(offset, offset));
            assert_eq!(shadow.blur_radius, px(0.));
            assert_eq!(shadow.spread_radius, px(0.));
            assert!(!shadow.inset);
        }
    }
}
