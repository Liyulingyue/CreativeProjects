#!/usr/bin/env python3
"""
Generate recipe card with specific content.
"""

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Dict, List, Optional

# Add scripts directory to path
sys.path.insert(0, str(Path(__file__).parent))

from generate_image import generate_image_with_optimized_prompt


def generate_recipe_card_prompt(
    recipe_name: str,
    ingredients: List[Dict],
    steps: List[Dict],
    style: str = "modern",
    color_scheme: str = "warm",
    layout_style: str = "circular"
) -> str:
    """
    Generate a prompt for recipe card with specific content.
    
    Args:
        recipe_name: Name of the recipe
        ingredients: List of ingredients with name and amount
        steps: List of steps with title and description
        style: Visual style
        color_scheme: Color scheme
        layout_style: Layout style
    
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
    
    # Build ingredients string
    ingredients_str = ", ".join([f"{ing['name']} ({ing['amount']})" for ing in ingredients[:8]])
    
    # Build steps string
    steps_str = ", ".join([f"Step {i+1}: {step['title']}" for i, step in enumerate(steps[:5])])
    
    # Build prompt
    base_prompt = f"Professional recipe card, vertical orientation, portrait format, "
    base_prompt += f"{styles.get(style, styles['modern'])}, "
    base_prompt += f"{color_schemes.get(color_scheme, color_schemes['warm'])}, "
    
    # Title
    base_prompt += f"title '{recipe_name}' at the top, "
    
    # Center area with food image
    base_prompt += "center area showing a beautiful photo of the finished dish, "
    
    # Layout style
    if layout_style == "circular":
        base_prompt += f"ingredients and steps arranged in a circular pattern around the center, "
        base_prompt += f"ingredients: {ingredients_str}, "
        base_prompt += f"steps: {steps_str}, "
        base_prompt += "each step grouped with its corresponding ingredients, "
        base_prompt += "steps ordered clockwise by cooking sequence, "
    elif layout_style == "grid":
        base_prompt += f"ingredients and steps arranged in a grid pattern around the center, "
        base_prompt += f"ingredients: {ingredients_str}, "
        base_prompt += f"steps: {steps_str}, "
    else:  # sequential
        base_prompt += f"ingredients and steps arranged sequentially around the center, "
        base_prompt += f"ingredients: {ingredients_str}, "
        base_prompt += f"steps: {steps_str}, "
    
    # Quality specifications
    base_prompt += "high quality, print-ready, 300 DPI, professional typography, "
    base_prompt += "clear hierarchy, balanced composition, readable text"
    
    return base_prompt


def generate_recipe_card_from_json(
    json_path: str,
    style: str = "modern",
    color_scheme: str = "warm",
    layout_style: str = "circular",
    api_key: Optional[str] = None,
    save_path: Optional[str] = None
) -> Dict:
    """
    Generate a recipe card from a JSON file.
    
    Args:
        json_path: Path to JSON file with recipe content
        style: Visual style
        color_scheme: Color scheme
        layout_style: Layout style
        api_key: API key
        save_path: Path to save the generated image
    
    Returns:
        Generation result
    """
    # Read JSON file
    with open(json_path, 'r', encoding='utf-8') as f:
        recipe_data = json.load(f)
    
    # Extract content
    recipe_name = recipe_data.get('recipe_name', recipe_data.get('name', 'Recipe'))
    ingredients = recipe_data.get('ingredients', [])
    steps = recipe_data.get('steps', [])
    
    # Generate prompt
    prompt = generate_recipe_card_prompt(
        recipe_name=recipe_name,
        ingredients=ingredients,
        steps=steps,
        style=style,
        color_scheme=color_scheme,
        layout_style=layout_style
    )
    
    print(f"Generated prompt for: {recipe_name}")
    print(f"Ingredients: {len(ingredients)}")
    print(f"Steps: {len(steps)}")
    print(f"\nPrompt:\n{prompt}\n")
    
    # Generate image
    result = generate_image_with_optimized_prompt(
        base_prompt=prompt,
        style=style,
        api_key=api_key,
        save_path=save_path
    )
    
    return result


def main():
    parser = argparse.ArgumentParser(description="Generate recipe card from JSON")
    parser.add_argument("json_path", help="Path to JSON file with recipe content")
    parser.add_argument("--style", default="modern",
                       choices=["modern", "rustic", "elegant", "playful"],
                       help="Visual style")
    parser.add_argument("--color-scheme", default="warm",
                       choices=["warm", "cool", "neutral", "vibrant"],
                       help="Color scheme")
    parser.add_argument("--layout-style", default="circular",
                       choices=["circular", "grid", "sequential"],
                       help="Layout style")
    parser.add_argument("--api-key", default=os.getenv("BAIDU_API_KEY"),
                       help="API key")
    parser.add_argument("--save-path", default=None,
                       help="Path to save the generated image")
    parser.add_argument("--prompt-only", action="store_true",
                       help="Only generate prompt, don't generate image")
    
    args = parser.parse_args()
    
    # Read JSON file
    with open(args.json_path, 'r', encoding='utf-8') as f:
        recipe_data = json.load(f)
    
    # Extract content
    recipe_name = recipe_data.get('recipe_name', recipe_data.get('name', 'Recipe'))
    ingredients = recipe_data.get('ingredients', [])
    steps = recipe_data.get('steps', [])
    
    # Generate prompt
    prompt = generate_recipe_card_prompt(
        recipe_name=recipe_name,
        ingredients=ingredients,
        steps=steps,
        style=args.style,
        color_scheme=args.color_scheme,
        layout_style=args.layout_style
    )
    
    print(f"Recipe: {recipe_name}")
    print(f"Ingredients: {len(ingredients)}")
    print(f"Steps: {len(steps)}")
    print(f"\nGenerated Prompt:\n{prompt}\n")
    
    if args.prompt_only:
        print("Prompt generated successfully. Use --save-path to generate image.")
        return
    
    # Generate image
    if not args.api_key:
        print("Error: API key not provided. Set BAIDU_API_KEY or use --api-key")
        return
    
    save_path = args.save_path or f"{recipe_name}_recipe_card.png"
    
    result = generate_image_with_optimized_prompt(
        base_prompt=prompt,
        style=args.style,
        api_key=args.api_key,
        save_path=save_path
    )
    
    if result["success"]:
        print(f"Image generated successfully!")
        print(f"Saved to: {result['save_path']}")
    else:
        print(f"Error: {result['error']}")


if __name__ == "__main__":
    main()
