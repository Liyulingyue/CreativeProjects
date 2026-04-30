import os
import ctypes
import base64
import argparse
import platform
import subprocess


def get_current_platform():
    return platform.system()


def get_screen_resolution():
    """获取当前屏幕分辨率（跨平台）"""
    current_platform = get_current_platform()
    
    if current_platform == "Windows":
        try:
            user32 = ctypes.windll.user32
            user32.SetProcessDPIAware()
            width = user32.GetSystemMetrics(0)
            height = user32.GetSystemMetrics(1)
            return width, height
        except Exception:
            return None, None
    
    elif current_platform == "Darwin":
        try:
            import subprocess
            result = subprocess.run(
                ["system_profiler", "SPDisplaysDataType"],
                capture_output=True,
                text=True
            )
            for line in result.stdout.split('\n'):
                if 'Resolution:' in line:
                    parts = line.split(':')[1].strip().split(' ')
                    width = int(parts[0])
                    height = int(parts[2])
                    return width, height
        except Exception:
            return None, None
    
    elif current_platform == "Linux":
        try:
            import subprocess
            result = subprocess.run(
                ["xrandr"],
                capture_output=True,
                text=True
            )
            for line in result.stdout.split('\n'):
                if '*' in line and 'x' in line:
                    parts = line.strip().split()[0].split('x')
                    width = int(parts[0])
                    height = int(parts[1])
                    return width, height
        except Exception:
            return None, None
    
    return None, None


def select_best_size(width, height):
    """根据屏幕宽高比选择最合适的图片规格"""
    if width is None or height is None:
        return "1024x1024"
    
    ratio = width / height
    
    # 常见的屏幕比例判断
    if ratio > 1.7:  # 16:9 左右
        return "1376x768"
    elif ratio > 1.4:  # 3:2 左右
        return "1264x848"
    elif ratio > 1.3:  # 4:3 左右
        return "1200x896"
    elif ratio < 0.6:  # 9:16 左右 (竖屏)
        return "768x1376"
    elif ratio < 0.75:  # 2:3 左右 (竖屏)
        return "848x1264"
    elif ratio < 0.8:  # 3:4 左右 (竖屏)
        return "896x1200"
    else:
        return "1024x1024"  # 默认正方形


def ensure_supported_platform():
    current_platform = get_current_platform()
    if current_platform not in {"Windows", "Darwin", "Linux"}:
        print("错误: 当前仅支持 Windows、macOS 和 Linux。")
        return False
    return True


def set_wallpaper_windows(abs_path):
    # SPI_SETDESKWALLPAPER = 20
    # SPIF_UPDATEINIFILE = 0x01 | SPIF_SENDWININICHANGE = 0x02
    ctypes.windll.user32.SystemParametersInfoW(20, 0, abs_path, 0x01 | 0x02)
    return True


def set_wallpaper_macos(abs_path):
    script = """
on run argv
    set wallpaperPath to POSIX file (item 1 of argv)
    tell application "System Events"
        set picture of every desktop to wallpaperPath
    end tell
end run
"""
    try:
        subprocess.run(
            ["osascript", "-e", script, abs_path],
            check=True,
            capture_output=True,
            text=True
        )
        return True
    except subprocess.CalledProcessError as error:
        error_message = error.stderr.strip() or error.stdout.strip() or str(error)
        print(f"macOS 设置壁纸失败: {error_message}")
        print("请确认 Terminal 或 Python 已在'系统设置 > 隐私与安全性 > 自动化'中获得控制 System Events 的权限。")
        return False


def set_wallpaper_linux(abs_path):
    """设置Linux桌面壁纸（支持多种桌面环境）"""
    import shutil
    
    # 检测桌面环境
    desktop_env = os.environ.get('XDG_CURRENT_DESKTOP', '').lower()
    session_desktop = os.environ.get('DESKTOP_SESSION', '').lower()
    
    # GNOME / Unity / Budgie / Pantheon
    if 'gnome' in desktop_env or 'unity' in desktop_env or 'budgie' in desktop_env or 'pantheon' in desktop_env:
        try:
            # 设置为缩放模式
            subprocess.run([
                'gsettings', 'set', 'org.gnome.desktop.background', 'picture-uri',
                f'file://{abs_path}'
            ], check=True, capture_output=True, text=True)
            # 设置暗色模式壁纸
            subprocess.run([
                'gsettings', 'set', 'org.gnome.desktop.background', 'picture-uri-dark',
                f'file://{abs_path}'
            ], check=True, capture_output=True, text=True)
            subprocess.run([
                'gsettings', 'set', 'org.gnome.desktop.background', 'picture-options', 'zoom'
            ], check=True, capture_output=True, text=True)
            return True
        except subprocess.CalledProcessError as error:
            print(f"GNOME 设置壁纸失败: {error}")
            return False
    
    # KDE Plasma
    elif 'kde' in desktop_env or 'plasma' in desktop_env or session_desktop == 'plasma':
        try:
            # 使用 qdbus6 或 qdbus
            script = f'''
var Desktops = desktops();
for (var i=0; i<Desktops.length; i++) {{
    var d = Desktops[i];
    d.wallpaperPlugin = "org.kde.image";
    d.currentConfigGroup = ["Wallpaper", "org.kde.image", "General"];
    d.writeConfig("Image", "file://{abs_path}");
}}
'''
            # 尝试 qdbus6 或 qdbus
            for cmd in ['qdbus6', 'qdbus']:
                if shutil.which(cmd):
                    result = subprocess.run(
                        [cmd, 'org.kde.plasmashell', '/PlasmaShell', 'org.kde.PlasmaShell.evaluateScript', script],
                        capture_output=True, text=True
                    )
                    if result.returncode == 0:
                        return True
            print("KDE 设置壁纸失败: 未找到 qdbus 或 qdbus6")
            return False
        except Exception as error:
            print(f"KDE 设置壁纸失败: {error}")
            return False
    
    # XFCE
    elif 'xfce' in desktop_env or session_desktop == 'xfce':
        try:
            # 获取当前工作区的属性名
            result = subprocess.run(
                ['xfconf-query', '-c', 'xfce4-desktop', '-l'],
                capture_output=True, text=True
            )
            for prop in result.stdout.split('\n'):
                if 'last-image' in prop or 'workspace' in prop:
                    subprocess.run([
                        'xfconf-query', '-c', 'xfce4-desktop', '-p', prop, '-s', abs_path
                    ], capture_output=True, text=True)
            return True
        except Exception as error:
            print(f"XFCE 设置壁纸失败: {error}")
            return False
    
    # Cinnamon
    elif 'cinnamon' in desktop_env:
        try:
            subprocess.run([
                'gsettings', 'set', 'org.cinnamon.desktop.background', 'picture-uri',
                f'file://{abs_path}'
            ], check=True, capture_output=True, text=True)
            subprocess.run([
                'gsettings', 'set', 'org.cinnamon.desktop.background', 'picture-options', 'zoom'
            ], check=True, capture_output=True, text=True)
            return True
        except subprocess.CalledProcessError as error:
            print(f"Cinnamon 设置壁纸失败: {error}")
            return False
    
    # MATE
    elif 'mate' in desktop_env:
        try:
            subprocess.run([
                'gsettings', 'set', 'org.mate.background', 'picture-filename',
                abs_path
            ], check=True, capture_output=True, text=True)
            return True
        except subprocess.CalledProcessError as error:
            print(f"MATE 设置壁纸失败: {error}")
            return False
    
    # 尝试使用通用工具 feh 或 nitrogen
    else:
        for tool in ['feh', 'nitrogen']:
            if shutil.which(tool):
                try:
                    if tool == 'feh':
                        subprocess.run(['feh', '--bg-fill', abs_path], check=True, capture_output=True, text=True)
                    else:
                        subprocess.run(['nitrogen', '--set-zoom-fill', abs_path, '--save'], check=True, capture_output=True, text=True)
                    return True
                except subprocess.CalledProcessError as error:
                    print(f"{tool} 设置壁纸失败: {error}")
                    return False
        
        print(f"错误: 无法设置壁纸。未识别的桌面环境: {desktop_env}")
        print("请安装 feh 或 nitrogen 工具，或使用支持的桌面环境 (GNOME/KDE/XFCE/Cinnamon/MATE)")
        return False


# 1. 设置壁纸的核心逻辑
def set_wallpaper(image_path):
    abs_path = os.path.abspath(image_path)
    if not os.path.exists(abs_path):
        print(f"错误: 路径 {abs_path} 不存在。")
        return False

    current_platform = get_current_platform()
    if current_platform == "Windows":
        success = set_wallpaper_windows(abs_path)
    elif current_platform == "Darwin":
        success = set_wallpaper_macos(abs_path)
    elif current_platform == "Linux":
        success = set_wallpaper_linux(abs_path)
    else:
        print(f"错误: 暂不支持的平台: {current_platform}")
        return False

    if not success:
        return False

    print(f"壁纸已成功设置为: {abs_path}")
    return True


# 2. 调用 API 生成图片
def generate_wallpaper(args):
    try:
        from openai import OpenAI
    except ImportError:
        print("错误: 缺少依赖 openai，请先执行 `pip install openai`。")
        return None

    # 处理图片尺寸
    image_size = args.size
    if image_size == "auto" or not image_size:
        # 自动探测屏幕分辨率
        width, height = get_screen_resolution()
        if width and height:
            image_size = select_best_size(width, height)
            print(f"检测到屏幕分辨率: {width}x{height} -> 使用图片规格: {image_size}")
        else:
            image_size = "1024x1024"
            print(f"无法检测屏幕分辨率，使用默认规格: {image_size}")
    else:
        print(f"使用指定的图片规格: {image_size}")

    # 使用 OpenAI Python SDK 访问兼容 OpenAI 协议的图像生成端点。
    client = OpenAI(
        api_key=args.api_key,
        base_url=args.base_url
    )
    
    print(f"正在生成图片，提示词: '{args.prompt}'...")
    try:
        response = client.images.generate(
            model=args.model,
            prompt=args.prompt,
            n=1,
            size=image_size,
            response_format="b64_json",
            extra_body={
                "seed": args.seed, 
                "use_pe": True, 
                "num_inference_steps": 8, 
                "guidance_scale": 1.0
            }
        )
        
        # 处理 Base64 数据
        b64_data = response.data[0].b64_json
        image_bytes = base64.b64decode(b64_data)
        
        if not os.path.exists(args.save_dir):
            os.makedirs(args.save_dir)
            
        # 使用固定文件名 wallpaper.png 避免存储空间无限扩大
        file_name = "wallpaper.png"
        image_path = os.path.join(args.save_dir, file_name)
        
        with open(image_path, 'wb') as handler:
            handler.write(image_bytes)
            
        return image_path
    except Exception as e:
        print(f"生成图片失败: {e}")
        return None

# 3. 主流程
def main():
    parser = argparse.ArgumentParser(description="AIWallpaper Skill - 百度文心 API 生成并设置 Windows/macOS 桌面壁纸")
    
    # 必须参数或通过环境变量获取
    parser.add_argument("--api_key", type=str, default=os.getenv("BAIDU_API_KEY"), help="API Key (也可通过环境变量 BAIDU_API_KEY 设置)")
    parser.add_argument("--prompt", type=str, help="提示词 (如果没有提供，将进入交互模式)")
    
    # 可选参数
    parser.add_argument("--base_url", type=str, default="https://aistudio.baidu.com/llm/lmapi/v3", help="API Base URL")
    parser.add_argument("--model", type=str, default="ernie-image-turbo", help="生成模型")
    parser.add_argument("--save_dir", type=str, default="assets", help="保存目录")
    parser.add_argument("--size", type=str, default="auto", help="图片尺寸 (例如: 1024x1024, 1376x768, 或 'auto' 自动检测)")
    parser.add_argument("--seed", type=int, default=42, help="随机种子")

    args = parser.parse_args()

    if not ensure_supported_platform():
        return

    # API Key 检查
    if not args.api_key:
        print("错误: 未提供 API Key。请通过 --api_key 参数或 BAIDU_API_KEY 环境变量提供。")
        return

    # 提示词检查
    if not args.prompt:
        args.prompt = input("请输入壁纸提示词 (例如 '赛博朋克风格的赛车'): ")
        if not args.prompt:
            print("错误: 提示词不能为空。")
            return
        
    image_path = generate_wallpaper(args)
    if image_path:
        set_wallpaper(image_path)

if __name__ == "__main__":
    main()
