use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LabelMap(pub HashMap<i32, String>);

impl LabelMap {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    pub fn from_json(json: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let labels: Vec<String> = serde_json::from_str(json)?;
        let map: HashMap<i32, String> = labels
            .into_iter()
            .enumerate()
            .map(|(i, label)| (i as i32, label))
            .collect();
        Ok(LabelMap(map))
    }

    pub fn get(&self, index: i32) -> Option<&str> {
        self.0.get(&index).map(|s| s.as_str())
    }

    pub fn get_or_default<'a>(&'a self, index: i32, default: &'a str) -> &'a str {
        self.get(index).unwrap_or(default)
    }
}

pub static IMAGENET_LABELS: &[&str] = &[
    "tench", "goldfish", "great white shark", "tiger shark", "hammerhead shark",
    "electric ray", "stingray", "cock", "hen", "ostrich", "brambling", "goldfinch",
    "house finch", "junco", "indigo bunting", "American robin", "bulbul", "jay",
    "magpie", "chickadee", "American dipper", "kite", "bald eagle", "vulture",
    "great grey owl", "fire salamander", "smooth newt", "newt", "spotted salamander",
    "axolotl", "American bullfrog", "tree frog", "tailed frog", "loggerhead sea turtle",
    "leatherback sea turtle", "mud turtle", "terrapin", "box turtle", "banded gecko",
    "green lizard", "American chameleon", "frilled lizard", "Gila monster", "green iguana",
    "Caribbean anole", "Madagascar day gecko", "whiptail", "keeled monitor lizard",
    "African monitor lizard", "Ghana mole rat", "golden mole rat", "vernal pool mole rat",
    "hedgehog", "tenrec", "moonrat", "least shrew", "short-tailed shrew", "smoky shrew",
    "pygmy shrew", "silky shrew", "water shrew", " Eurasian harvest mouse", "deer mouse",
    "dormouse", "hedgehog spiny mouse", "Lork's desert mouse", "African grass mouse",
    "big brown bat", "little brown bat", "pale bat", "silver-haired bat", "hoary bat",
    "red bat", "sernet", "squash bat", "tunnel of snakes", "tomb bat", "Mauritian tomb bat",
    "flappet lark", "sunbird", "hummingbird", "hornero", "bush warbler", "sedge warbler",
    "grasshopper warbler", "spotted flycatcher", "robins", "Eurasian jay", "Eurasian magpie",
    "black-billed magpie", "yellow-billed magpie", "fork-tailed flycatcher", "ashkenazi bill",
    "European rook", "cor上的 mon crow", "crow", "raven", "black capped chickadee", "rook", "cob",
    "potoo", "great grey owl", "snowy owl", "barred owl", "horned owl", "pelagic cormorant",
    "great cormorant", "double-crested cormorant", "American cormorant", "red-faced cormorant",
    "gannet", "masked booby", "brown booby", "red-footed booby", "northern gannet",
    "Atlantic puffin", "horned puffin", "tufted puffin", "frigatebird", "great frigatebird",
    "lesser frigatebird", "African skimmer", "black skimmer", "black leich", "Ross's goose",
    "snow goose", "Brent goose", "Cackling Goose", "Canada goose", "goose", "swan goose",
    "swan", "tundra swan", "whistling duck", "spur-winged goose", "pink-footed goose",
    "white-fronted goose", "longtail", "Ruddy shelduck", "shell duck", "Muscovy duck",
    "gadwall", "falcated duck", "Eurasis wigeon", "American wigeon", "American black duck",
    "Mallard", "Northern shoveler", "Northern pintail", "Green-winged teal", "canvasback",
    "redhead", "tufted duck", "ring-necked duck", "greater scaup", "lesser scaup",
    "Steller's eider", "king eider", "harlequin duck", "surf scoter", "black scoter",
    "velvet scoter", "steamer duck", "Welse's duck", "oldsquaw", "horned grebe",
    "red-necked grebe", "eared grebe", "least grebe", "pied-billed grebe", "western grebe",
    "Clark's grebe", "horgrebe", "northern fulmar", "storm petrel", "white-faced storm petrel",
    "black-winged stilt", "avocet", "recorder", "gouldian finch", "java sparrow",
    "Japanese quail", "common quail", "rainbow lorikeet", "cockatiel", "budgerigar",
    "lesser prairie chicken", "great bustard", "chickens", "turkeys", "black grouse",
    "ptarmigan", "ruffed grouse", "prairie chicken", "peacock", "quelea", "cox",
    "northern shoveler", "buck", "fallow deer", "roan antelope", "impala", "gazelle",
    "dromedary", "bactrian camel", "llama", "weasel", "mink", "European polecat",
    "black-footed ferret", "fisher", "pekan", "brown bear", "ice bear", "sloth bear",
    "sun bear", "coati", "red panda", "mustelid", "wolverine", "badger", "river otter",
    "sea otter", "American mink", "European mink", "weasel", "least weasel", "stoat",
    "longtail weasel", "stripe-faced delta", "american_river_otter", "smooth-coated otter",
    "spotted-necked otter", "sea otter", "clawless otter", "aardwolf", "African wild dog",
    "gray wolf", "coyote", "dingo", "dhole", "African hunting dog", "hyena", "red fox",
    "kit fox", "fennec fox", "grey seal", "harbor seal", "elephant seal", "jackal", "meerkat",
    "Star", "sorrel", "arabian horse", "Przewalski's horse", "cattle", "bull", "water buffalo",
    "bison", "ox", "ram", "bighorn", "ibex", "markhor", "domestic F",
    "-crested", "squirrel", "chipmunk", "marmot", "prairie dog", "gopher", "beaver",
    "mole", "hedgehog", "moonrat", "shrew", "Tree shrew", "colobus", "proboscis monkey",
    "macaque", "langur", "black snub-nosed monkey", "dol", "proboscis", "marmoset",
    "capuchin", "squirrel monkey", "owl monkey", "night monkey", "titi", "sakis", "uakari",
    "owl monkey", "Howler", "spider monkey", "squirrel monkey", "talapoin", "green monkey",
    "vervet", "Eule", "baboon", "gelada", "gibbons", "lar", "pied-a-gr", "pied sifaka",
    "Indri", "potto", "loris", "bushbaby", "tree shrew", "Ring-tailed Lemur", "galago",
    "agile galago", "slow loris", "potto", "angwantoro", "goldenPot", "sunda galago",
    "galago", "bonobo", "chimpanzee", "orangutan", "gorilla", "gibbons", "siamang",
    "Hand", "foot", "leg", "wing", "flipper", "arm", "fist", "tooth", "tongue",
    "tail", "heart", "gills", "puls", "liver", "moon", "tail", "fins", "feathers",
    "antlers", "horns", "hooves", "paws", "hand", "foot", "leg", "wing", "flipper",
    "arm", "beak", "eye", "brain", "tooth", "tongue", "tail", "heart", "gills",
    "wings", "feathers", "scales", "teeth", "claws", "horns", "hooves", "paws",
];

pub static COCO_LABELS: &[&str] = &[
    "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat",
    "traffic light", "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat",
    "dog", "horse", "sheep", "cow", "elephant", "bear", "zebra", "giraffe", "backpack",
    "umbrella", "handbag", "tie", "suitcase", "frisbee", "skis", "snowboard", "sports ball",
    "kite", "baseball bat", "baseball glove", "skateboard", "surfboard", "tennis racket",
    "bottle", "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple",
    "sandwich", "orange", "broccoli", "carrot", "hot dog", "pizza", "donut", "cake",
    "chair", "couch", "potted plant", "bed", "dining table", "toilet", "tv", "laptop",
    "mouse", "remote", "keyboard", "cell phone", "microwave", "oven", "toaster", "sink",
    "refrigerator", "book", "clock", "vase", "scissors", "teddy bear", "hair drier",
    "toothbrush",
];

pub static HAND_LABELS: &[&str] = &["Left", "Right"];

pub static FACE_BLENDSHAPE_NAMES: &[&str] = &[
    "neutral", "browDown_L", "browDown_R", "browInnerUp", "browOuterUp_L", "browOuterUp_R",
    "cheekPuff", "cheekSquint_L", "cheekSquint_R", "eyeBlink_L", "eyeBlink_R", "eyeLookDown_L",
    "eyeLookDown_R", "eyeLookIn_L", "eyeLookIn_R", "eyeLookOut_L", "eyeLookOut_R",
    "eyeLookUp_L", "eyeLookUp_R", "eyeSquint_L", "eyeSquint_R", "eyeWide_L", "eyeWide_R",
    "jawForward", "jawLeft", "jawRight", "jawOpen", "mouthClose", "mouthFunnel", "mouthPucker",
    "mouthLeft", "mouthRight", "mouthSmile_L", "mouthSmile_R", "mouthSmile", "mouthFrown_L",
    "mouthFrown_R", "mouthDimple_L", "mouthDimple_R", "mouthStretch_L", "mouthStretch_R",
    "mouthRoll_L", "mouthRoll_R", "mouthShrugLower", "mouthShrugUpper", "mouthPress_L",
    "mouthPress_R", "mouthLowerDown_L", "mouthLowerDown_R", "mouthUpperUp_L", "mouthUpperUp_R",
    "nasalFlare_R", "nasalFlare_L", "noseSneer_R", "noseSneer_L", "cheekPuff", "cheekSuction_R",
    "cheekSuction_L", "tongueOut",
];

pub fn get_imagenet_label(index: usize) -> Option<&'static str> {
    IMAGENET_LABELS.get(index).copied()
}

pub fn get_coco_label(index: usize) -> Option<&'static str> {
    COCO_LABELS.get(index).copied()
}

pub fn get_hand_label(index: usize) -> Option<&'static str> {
    HAND_LABELS.get(index).copied()
}

pub fn get_face_blendshape_name(index: usize) -> Option<&'static str> {
    FACE_BLENDSHAPE_NAMES.get(index).copied()
}
