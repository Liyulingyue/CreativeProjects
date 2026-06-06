import sensor from '@ohos.sensor';

export default {
    data: {
        xVal: '0.000',
        yVal: '0.000',
        zVal: '0.000',
        xColor: '#44ccff',
        yColor: '#44ff88',
        zColor: '#ffaa44',
        isActive: false,
        errMsg: ''
    },
    sensorSubscription: null,
    onInit() {
        this.autoStart();
    },
    onDestroy() {
        this.stopSensor();
    },
    autoStart() {
        this.toggleSensor();
    },
    getAxisColor(val) {
        const absVal = Math.abs(val);
        if (absVal < 2) {
            return '#44ccff';
        } else if (absVal < 5) {
            return '#44ff88';
        } else {
            return '#ff4444';
        }
    },
    toggleSensor() {
        if (this.isActive) {
            this.stopSensor();
        } else {
            this.startSensor();
        }
    },
    startSensor() {
        this.errMsg = '';
        try {
            // 先检查传感器是否可用
            sensor.isSensorTypeSupported(sensor.SensorType.ACCELEROMETER, (err, isSupported) => {
                if (err || !isSupported) {
                    this.errMsg = '当前设备不支持加速计';
                    console.error('Accelerometer not supported: ' + JSON.stringify(err));
                    return;
                }
                
                // 传感器可用，开始订阅
                try {
                    this.sensorSubscription = sensor.subscribeAccelerometer(
                        {
                            interval: 'game'
                        },
                        (data) => {
                            const x = data.x;
                            const y = data.y;
                            const z = data.z;
                            this.xVal = x.toFixed(3);
                            this.yVal = y.toFixed(3);
                            this.zVal = z.toFixed(3);
                            this.xColor = this.getAxisColor(x);
                            this.yColor = this.getAxisColor(y);
                            this.zColor = this.getAxisColor(z);
                        }
                    );
                    this.isActive = true;
                } catch (subErr) {
                    this.errMsg = '加速计启动失败';
                    this.isActive = false;
                    console.error('Subscribe error: ' + JSON.stringify(subErr));
                }
            });
        } catch (err) {
            this.errMsg = this.$t('strings.sensor_err');
            this.isActive = false;
            console.error('Sensor error: ' + JSON.stringify(err));
        }
    },
    stopSensor() {
        if (this.sensorSubscription !== null) {
            try {
                sensor.unsubscribeAccelerometer();
            } catch (err) {
                console.error('Unsubscribe error: ' + JSON.stringify(err));
            }
            this.sensorSubscription = null;
        }
        this.isActive = false;
    }
};
