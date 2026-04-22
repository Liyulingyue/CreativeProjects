from smbus2 import SMBus
import math


class YB_Pcb_Car:
    def __init__(self, address=0x16, i2c_bus=1):
        self._addr = address
        self._device = SMBus(i2c_bus)

    def write_u8(self, reg, data):
        self._device.write_byte_data(self._addr, reg, data)

    def write_array(self, reg, data):
        self._device.write_i2c_block_data(self._addr, reg, data)

    def Ctrl_Car(self, l_dir, l_speed, r_dir, r_speed):
        reg = 0x01
        data = [l_dir, l_speed, r_dir, r_speed]
        self.write_array(reg, data)

    def Control_Car(self, speed1, speed2):
        dir1 = 0 if speed1 < 0 else 1
        dir2 = 0 if speed2 < 0 else 1
        self.Ctrl_Car(dir1, int(abs(speed1)), dir2, int(abs(speed2)))

    def Car_Run(self, speed1, speed2):
        self.Ctrl_Car(1, speed1, 1, speed2)

    def Car_Stop(self):
        self.write_u8(0x02, 0x00)

    def Car_Back(self, speed1, speed2):
        self.Ctrl_Car(0, speed1, 0, speed2)

    def Car_Left(self, speed1, speed2):
        self.Ctrl_Car(0, speed1, 1, speed2)

    def Car_Right(self, speed1, speed2):
        self.Ctrl_Car(1, speed1, 0, speed2)

    def Car_Spin_Left(self, speed1, speed2):
        self.Ctrl_Car(0, speed1, 1, speed2)

    def Car_Spin_Right(self, speed1, speed2):
        self.Ctrl_Car(1, speed1, 0, speed2)

    def Ctrl_Servo(self, id, angle):
        angle = max(0, min(180, angle))
        self.write_array(0x03, [id, angle])

    def close(self):
        self._device.close()
