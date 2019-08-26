use sh1106::{
    interface::DisplayInterface, mode::displaymode::DisplayModeTrait, properties::DisplayProperties,
};

pub struct DataMode<DI>
where
    DI: DisplayInterface,
{
    properties: DisplayProperties<DI>,
}

impl<DI> DisplayModeTrait<DI> for DataMode<DI>
where
    DI: DisplayInterface,
{
    fn new(properties: DisplayProperties<DI>) -> Self {
        DataMode { properties }
    }

    fn release(self) -> DisplayProperties<DI> {
        self.properties
    }
}

impl<DI: DisplayInterface> DataMode<DI> {
    pub fn new(properties: DisplayProperties<DI>) -> Self {
        DataMode { properties }
    }

    pub fn init(&mut self) -> Result<(), DI::Error> {
        self.properties.init_column_mode()
    }

    pub fn clear(&mut self) -> Result<(), DI::Error> {
        self.draw(&[0; 1024])
    }

    pub fn draw(&mut self, data: &[u8]) -> Result<(), DI::Error> {
        let display_size = self.properties.get_size();

        let (display_width, display_height) = display_size.dimensions();
        let column_offset = display_size.column_offset();
        self.properties.set_draw_area(
            (column_offset, 0),
            (display_width + column_offset, display_height),
        )?;

        let length = (display_width as usize) * (display_height as usize) / 8;

        self.properties.draw(&data[..length])
    }
}
