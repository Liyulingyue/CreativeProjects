# -*- coding: utf-8 -*-
"""
AI配置管理模块
负责API配置的加载和管理
"""


def load_api_config():
    """
    Load API configuration from .env file
    """
    try:
        import os
        from dotenv import load_dotenv

        # Load .env file
        load_dotenv()

        api_key = os.getenv('API_KEY', '')
        base_url = os.getenv('BASE_URL', '')
        model = os.getenv('MODEL', '')

        return api_key, base_url, model
    except ImportError:
        # If python-dotenv is not installed, try to read .env file manually
        try:
            import os
            env_file = os.path.join(os.path.dirname(__file__), '..', '.env')

            if os.path.exists(env_file):
                config = {}
                with open(env_file, 'r', encoding='utf-8') as f:
                    for line in f:
                        line = line.strip()
                        if line and not line.startswith('#'):
                            key, value = line.split('=', 1)
                            config[key.strip()] = value.strip()

                return (
                    config.get('API_KEY', ''),
                    config.get('BASE_URL', ''),
                    config.get('MODEL', '')
                )
            else:
                return '', '', ''
        except Exception as e:
            return '', '', ''
    except Exception as e:
        return '', '', ''


def validate_and_start(api_key, base_url, model):
    """
    Validate API configuration and start survey (optional)
    """
    # Store configuration for later use (even if empty)
    global current_api_config
    current_api_config = {
        'api_key': api_key,
        'base_url': base_url,
        'model': model
    }

    # Always proceed to survey tab
    import gradio as gr
    return gr.update(selected=1)


# Global variable to store API configuration
current_api_config = {}