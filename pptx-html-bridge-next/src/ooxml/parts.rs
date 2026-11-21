//! 空白模板部件（python-pptx 引导生成，开发期嵌入）。写入时以此打底。
#![allow(dead_code)]

pub const CT_XML: &str = include_str!("../../assets/template/parts/[Content_Types].xml");
pub const ROOT_RELS: &str = include_str!("../../assets/template/parts/_rels/.rels");
pub const PRES_XML: &str = include_str!("../../assets/template/parts/ppt/presentation.xml");
pub const PRES_RELS: &str = include_str!("../../assets/template/parts/ppt/_rels/presentation.xml.rels");
pub const MASTER_XML: &str = include_str!("../../assets/template/parts/ppt/slideMasters/slideMaster1.xml");
pub const MASTER_RELS: &str = include_str!("../../assets/template/parts/ppt/slideMasters/_rels/slideMaster1.xml.rels");
pub const THEME_XML: &str = include_str!("../../assets/template/parts/ppt/theme/theme1.xml");
pub const PRES_PROPS: &str = include_str!("../../assets/template/parts/ppt/presProps.xml");
pub const VIEW_PROPS: &str = include_str!("../../assets/template/parts/ppt/viewProps.xml");
pub const TABLE_STYLES: &str = include_str!("../../assets/template/parts/ppt/tableStyles.xml");
pub const DOC_CORE: &str = include_str!("../../assets/template/parts/docProps/core.xml");
pub const DOC_APP: &str = include_str!("../../assets/template/parts/docProps/app.xml");
pub const PRINTER_BIN: &[u8] = include_bytes!("../../assets/template/parts/ppt/printerSettings/printerSettings1.bin");
pub const THUMB_JPEG: &[u8] = include_bytes!("../../assets/template/parts/docProps/thumbnail.jpeg");

pub const LAYOUT_XML: [&str; 11] = [
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout1.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout2.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout3.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout4.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout5.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout6.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout7.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout8.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout9.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout10.xml"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/slideLayout11.xml"),
];

pub const LAYOUT_RELS: [&str; 11] = [
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout1.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout2.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout3.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout4.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout5.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout6.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout7.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout8.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout9.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout10.xml.rels"),
    include_str!("../../assets/template/parts/ppt/slideLayouts/_rels/slideLayout11.xml.rels"),
];
