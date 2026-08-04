use mixpiperust::{PipelineBuilder, Resize, Normalize};

fn main() {
    let pipeline = PipelineBuilder::new()
        .add_node(Resize::new(192, 256))
        .add_node(Normalize::imagenet())
        .build();

    println!("MixPipeRust pipeline created with {} nodes", pipeline.nodes());
    println!("Supported media types: Image, Audio, Text, Video");
}
