from flask import Flask, render_template, jsonify, request
import system_info
import subprocess

app = Flask(__name__)

@app.route("/")
def index():
    return render_template("index.html")

@app.route("/api/status")
def status():
    return jsonify({
        "cpu": system_info.get_cpu_info(),
        "memory": system_info.get_memory_info(),
        "ips": system_info.get_ip_addresses()
    })

@app.route("/api/wifi/list")
def wifi_list():
    return jsonify(system_info.get_wifi_networks())

@app.route("/api/wifi/connect", methods=["POST"])
def wifi_connect():
    data = request.json
    ssid = data.get("ssid")
    password = data.get("password")
    
    if not ssid:
        return jsonify({"error": "SSID is required"}), 400
    
    try:
        # Connect using nmcli
        # Note: This might block or fail if the password is wrong
        cmd = ["nmcli", "dev", "wifi", "connect", ssid]
        if password:
            cmd.extend(["password", password])
        
        subprocess.check_call(cmd)
        return jsonify({"success": True})
    except subprocess.CalledProcessError as e:
        return jsonify({"error": f"Failed to connect: {str(e)}"}), 500

if __name__ == "__main__":
    # In mixed mode, we usually serve on the AP's IP. 
    # 0.0.0.0 listens on all interfaces.
    app.run(host="0.0.0.0", port=5000)
