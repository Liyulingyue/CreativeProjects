#!/usr/bin/env python3
"""
Generate recipe card templates with customizable layouts.
"""

import argparse
import json
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple

def generate_recipe_prompt(
    cuisine_type: str,
    style: str = "modern",
    color_scheme: str = "warm",
    center_ratio: float = 0.4,
    include_ingredients: bool = True,
    include_steps: bool = True,
    num_steps: int = 5,
    custom_elements: Optional[List[str]] = None,
    layout_style: str = "circular"
) -> str:
    """
    Generate an optimized prompt for recipe card generation.
    
    Args:
        cuisine_type: Type of cuisine (e.g., Italian, Chinese, Mexican)
        style: Visual style (modern, rustic, elegant, playful)
        color_scheme: Color scheme (warm, cool, neutral, vibrant)
        center_ratio: Ratio of center area for user content
        include_ingredients: Whether to include ingredients list
        include_steps: Whether to include cooking steps
        num_steps: Number of steps to include
        custom_elements: Additional custom elements to include
        layout_style: Layout style (circular, grid, sequential)
    
    Returns:
        Optimized prompt string
    """
    
    # Style definitions
    styles = {
        "modern": "minimalist design, clean lines, sans-serif fonts, white space, geometric elements",
        "rustic": "vintage paper texture, handwritten-style fonts, warm earth tones, aged look, artisan feel",
        "elegant": "gourmet presentation, serif fonts, gold accents, premium materials, sophisticated",
        "playful": "bright colors, rounded shapes, playful typography, cartoon elements, energetic"
    }
    
    # Color scheme definitions
    color_schemes = {
        "warm": "warm earth tones, golden accents, cozy atmosphere",
        "cool": "cool blues and greens, calm atmosphere, fresh feel",
        "neutral": "neutral grays and beiges, professional, balanced",
        "vibrant": "bright, saturated colors, energetic, eye-catching"
    }
    
    # Base prompt - vertical orientation
    base_prompt = f"Professional recipe card template, vertical orientation, portrait format, {styles.get(style, styles['modern'])}, "
    base_prompt += f"{color_schemes.get(color_scheme, color_schemes['warm'])}, "
    base_prompt += f"{cuisine_type} cuisine recipe layout, "
    
    # Center area specification
    center_size = int(center_ratio * 100)
    base_prompt += f"circular center area taking {center_size}% of total space, "
    base_prompt += "clear boundaries, completely empty for user food photo, "
    
    # Layout style for ingredients and steps
    if layout_style == "circular":
        base_prompt += "cooking steps with their corresponding ingredients arranged in a circular pattern around the center area, "
        base_prompt += "each step positioned radially around the center with its required ingredients nearby, "
        base_prompt += "steps ordered clockwise by cooking sequence, "
        base_prompt += "ingredients visually grouped with their associated step, "
    elif layout_style == "grid":
        base_prompt += "cooking steps with their corresponding ingredients arranged in a grid pattern around the center area, "
        base_prompt += "each step grouped with its required ingredients, "
        base_prompt += "steps ordered by cooking sequence, "
    else:  # sequential
        base_prompt += "cooking steps with their corresponding ingredients arranged sequentially around the center area, "
        base_prompt += "each step grouped with its required ingredients, "
        base_prompt += "steps ordered by cooking sequence, "
    
    # Surrounding elements
    elements = []
    if include_ingredients and include_steps:
        elements.append(f"{num_steps} cooking steps each with its corresponding ingredients arranged around center")
    elif include_ingredients:
        elements.append("ingredient list with measurements")
    elif include_steps:
        elements.append(f"step-by-step cooking instructions ({num_steps} steps)")
    
    if elements:
        base_prompt += f"with {', '.join(elements)}, "
    
    # Custom elements
    if custom_elements:
        base_prompt += f"also include {', '.join(custom_elements)}, "
    
    # Quality specifications
    base_prompt += "high quality, print-ready, 300 DPI, professional typography, "
    base_prompt += "clear hierarchy, balanced composition, readable text, "
    base_prompt += "visual flow following clockwise cooking order"
    
    return base_prompt

def generate_layout_specification(
    center_ratio: float = 0.4,
    include_ingredients: bool = True,
    include_steps: bool = True,
    num_steps: int = 5,
    layout_style: str = "circular"
) -> Dict:
    """
    Generate layout specification for a recipe card.
    
    Args:
        center_ratio: Ratio of center area
        include_ingredients: Whether to include ingredients
        include_steps: Whether to include steps
        num_steps: Number of steps
        layout_style: Layout style (circular, grid, sequential)
    
    Returns:
        Layout specification dictionary
    """
    layout = {
        "total_size": {"width": 896, "height": 1200},
        "orientation": "portrait",
        "center_area": {
            "width_percent": center_ratio * 100,
            "height_percent": center_ratio * 100,
            "position": "center",
            "shape": "circular",
            "content": "user_food_photo"
        },
        "layout_style": layout_style,
        "sections": []
    }
    
    # Title section
    layout["sections"].append({
        "name": "title",
        "position": "top",
        "height_percent": 12,
        "content": "recipe_name",
        "typography": "large_decorative"
    })
    
    # Ingredients section - arranged around center
    if include_ingredients:
        layout["sections"].append({
            "name": "ingredients",
            "position": "around_center",
            "arrangement": "circular",
            "content": "ingredient_list_ordered_by_usage",
            "typography": "clear_readable",
            "order": "clockwise_by_usage_sequence"
        })
    
    # Steps section - arranged around ingredients
    if include_steps:
        layout["sections"].append({
            "name": "steps",
            "position": "around_ingredients",
            "arrangement": "circular",
            "content": "cooking_steps_ordered",
            "num_items": num_steps,
            "typography": "numbered_list",
            "order": "clockwise_by_step_sequence"
        })
    
    # Info section
    layout["sections"].append({
        "name": "info",
        "position": "bottom",
        "height_percent": 8,
        "content": "prep_time_cook_time_servings",
        "typography": "small_clear"
    })
    
    return layout

def generate_negative_prompt() -> str:
    """
    Generate negative prompt for recipe card generation.
    
    Returns:
        Negative prompt string
    """
    return (
        "food in center area, cluttered layout, unreadable text, "
        "poor composition, unbalanced elements, low contrast text, "
        "blurry, low quality, distorted, disfigured, deformed, "
        "extra limbs, bad anatomy, bad proportions, watermark, "
        "signature, text, logo, cropped, worst quality, low resolution"
    )

def create_recipe_card_template(
    cuisine_type: str,
    style: str = "modern",
    color_scheme: str = "warm",
    center_ratio: float = 0.4,
    include_ingredients: bool = True,
    include_steps: bool = True,
    num_steps: int = 5,
    custom_elements: Optional[List[str]] = None,
    layout_style: str = "circular",
    output_dir: Optional[str] = None
) -> Dict:
    """
    Create a complete recipe card template specification.
    
    Args:
        cuisine_type: Type of cuisine
        style: Visual style
        color_scheme: Color scheme
        center_ratio: Ratio of center area
        include_ingredients: Whether to include ingredients
        include_steps: Whether to include steps
        num_steps: Number of steps
        custom_elements: Additional custom elements
        layout_style: Layout style for ingredients around center
        output_dir: Directory to save template files
    
    Returns:
        Complete template specification
    """
    
    # Generate prompt
    prompt = generate_recipe_prompt(
        cuisine_type=cuisine_type,
        style=style,
        color_scheme=color_scheme,
        center_ratio=center_ratio,
        include_ingredients=include_ingredients,
        include_steps=include_steps,
        num_steps=num_steps,
        custom_elements=custom_elements,
        layout_style=layout_style
    )
    
    # Generate layout
    layout = generate_layout_specification(
        center_ratio=center_ratio,
        include_ingredients=include_ingredients,
        include_steps=include_steps,
        num_steps=num_steps,
        layout_style=layout_style
    )
    
    # Generate negative prompt
    negative_prompt = generate_negative_prompt()
    
    # Create template specification
    template = {
        "name": f"{cuisine_type.lower().replace(' ', '_')}_recipe_card",
        "description": f"{cuisine_type} recipe card template with {style} style",
        "cuisine_type": cuisine_type,
        "style": style,
        "color_scheme": color_scheme,
        "layout_style": layout_style,
        "image_size": {"width": 896, "height": 1200},
        "prompts": {
            "main": prompt,
            "negative": negative_prompt,
            "style_variations": {
                "modern": generate_recipe_prompt(cuisine_type, "modern", color_scheme, center_ratio, layout_style=layout_style),
                "rustic": generate_recipe_prompt(cuisine_type, "rustic", color_scheme, center_ratio, layout_style=layout_style),
                "elegant": generate_recipe_prompt(cuisine_type, "elegant", color_scheme, center_ratio, layout_style=layout_style),
                "playful": generate_recipe_prompt(cuisine_type, "playful", color_scheme, center_ratio, layout_style=layout_style)
            }
        },
        "layout": layout,
        "customization_options": {
            "cuisine_types": ["Italian", "Chinese", "Mexican", "Japanese", "Indian", "French", "Thai", "Mediterranean"],
            "styles": ["modern", "rustic", "elegant", "playful"],
            "color_schemes": ["warm", "cool", "neutral", "vibrant"],
            "center_ratios": [0.3, 0.4, 0.5, 0.6],
            "num_steps": [3, 5, 7, 10],
            "layout_styles": ["circular", "grid", "sequential"]
        },
        "usage_instructions": {
            "step1": "Generate image using the main prompt",
            "step2": "Verify center area is clear for user content",
            "step3": "User places their food photo in the center area",
            "step4": "Add recipe title, ingredients, and steps text",
            "step5": "Final recipe card is ready to share"
        }
    }
    
    # Save template if output directory specified
    if output_dir:
        output_path = Path(output_dir)
        output_path.mkdir(parents=True, exist_ok=True)
        
        template_file = output_path / f"{template['name']}.json"
        with open(template_file, 'w') as f:
            json.dump(template, f, indent=2)
        
        # Save prompt file
        prompt_file = output_path / f"{template['name']}_prompts.txt"
        with open(prompt_file, 'w') as f:
            f.write(f"Main Prompt:\n{prompt}\n\n")
            f.write(f"Negative Prompt:\n{negative_prompt}\n\n")
            f.write("Style Variations:\n")
            for style_name, style_prompt in template["prompts"]["style_variations"].items():
                f.write(f"\n{style_name.title()}:\n{style_prompt}\n")
        
        template["files"] = {
            "template": str(template_file),
            "prompts": str(prompt_file)
        }
    
    return template

def main():
    parser = argparse.ArgumentParser(description="Generate recipe card templates")
    parser.add_argument("cuisine_type", help="Type of cuisine (e.g., Italian, Chinese)")
    parser.add_argument("--style", default="modern", 
                       choices=["modern", "rustic", "elegant", "playful"],
                       help="Visual style")
    parser.add_argument("--color-scheme", default="warm",
                       choices=["warm", "cool", "neutral", "vibrant"],
                       help="Color scheme")
    parser.add_argument("--center-ratio", type=float, default=0.4,
                       help="Ratio of center area (0.0 to 1.0)")
    parser.add_argument("--no-ingredients", action="store_true",
                       help="Exclude ingredients list")
    parser.add_argument("--no-steps", action="store_true",
                       help="Exclude cooking steps")
    parser.add_argument("--num-steps", type=int, default=5,
                       help="Number of cooking steps")
    parser.add_argument("--custom-elements", nargs="+",
                       help="Additional custom elements to include")
    parser.add_argument("--layout-style", default="circular",
                       choices=["circular", "grid", "sequential"],
                       help="Layout style for ingredients around center")
    parser.add_argument("--output-dir", 
                       help="Directory to save template files")
    parser.add_argument("--json", action="store_true",
                       help="Output as JSON")
    
    args = parser.parse_args()
    
    # Create template
    template = create_recipe_card_template(
        cuisine_type=args.cuisine_type,
        style=args.style,
        color_scheme=args.color_scheme,
        center_ratio=args.center_ratio,
        include_ingredients=not args.no_ingredients,
        include_steps=not args.no_steps,
        num_steps=args.num_steps,
        custom_elements=args.custom_elements,
        layout_style=args.layout_style,
        output_dir=args.output_dir
    )
    
    if args.json:
        print(json.dumps(template, indent=2))
    else:
        print("Recipe Card Template")
        print("=" * 40)
        print(f"Cuisine: {args.cuisine_type}")
        print(f"Style: {args.style}")
        print(f"Color Scheme: {args.color_scheme}")
        print(f"Center Area: {args.center_ratio*100}%")
        print()
        print("Generated Prompt:")
        print(template["prompts"]["main"])
        print()
        print("Negative Prompt:")
        print(template["prompts"]["negative"])
        
        if args.output_dir:
            print()
            print("Files saved:")
            for file_type, file_path in template.get("files", {}).items():
                print(f"  {file_type}: {file_path}")

if __name__ == "__main__":
    main()
