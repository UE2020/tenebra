#include <fcntl.h>
#include <linux/uinput.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

typedef struct {
    int touch_fd;
    int scroll_fd;
    int pen_fd;
    int wheel_x;
    int wheel_y;
} MultiTouchSimulator;

void emit_event(int uinput_fd, int type, int code, int value) {
    struct input_event ev = {0};
    ev.type = type;
    ev.code = code;
    ev.value = value;
    ev.time.tv_sec = 0;
    ev.time.tv_usec = 0;

    if (write(uinput_fd, &ev, sizeof(ev)) == -1) {
        perror("Error writing event");
    }
}

int setup_devices(MultiTouchSimulator* simulator) {
    {
        struct uinput_user_dev uidev = {0};

        strncpy(uidev.name, "Tenebra Multi-Touch Device", UINPUT_MAX_NAME_SIZE);
        uidev.id.bustype = BUS_USB;
        uidev.id.vendor = 0x1234;
        uidev.id.product = 0x5678;
        uidev.id.version = 1;

        ioctl(simulator->touch_fd, UI_SET_EVBIT, EV_SYN);
        ioctl(simulator->touch_fd, UI_SET_EVBIT, EV_ABS);
        ioctl(simulator->touch_fd, UI_SET_ABSBIT, ABS_MT_SLOT);
        ioctl(simulator->touch_fd, UI_SET_ABSBIT, ABS_MT_POSITION_X);
        ioctl(simulator->touch_fd, UI_SET_ABSBIT, ABS_MT_POSITION_Y);
        ioctl(simulator->touch_fd, UI_SET_ABSBIT, ABS_MT_TRACKING_ID);
        ioctl(simulator->touch_fd, UI_SET_PROPBIT, INPUT_PROP_DIRECT);

        uidev.absmin[ABS_MT_POSITION_X] = 0;
        uidev.absmax[ABS_MT_POSITION_X] = 2000;
        uidev.absmin[ABS_MT_POSITION_Y] = 0;
        uidev.absmax[ABS_MT_POSITION_Y] = 2000;

        uidev.absmin[ABS_MT_SLOT] = 0;
        uidev.absmax[ABS_MT_SLOT] = 9;

        uidev.absmin[ABS_MT_TRACKING_ID] = 0;
        uidev.absmax[ABS_MT_TRACKING_ID] = 65535;

        if (write(simulator->touch_fd, &uidev, sizeof(uidev)) == -1) {
            perror("Error setting up device");
            return -1;
        }

        if (ioctl(simulator->touch_fd, UI_DEV_CREATE) == -1) {
            perror("Error creating uinput device");
            return -1;
        }
    }

    {
        struct uinput_user_dev uidev = {0};

        strncpy(uidev.name, "Tenebra Scroll Device", UINPUT_MAX_NAME_SIZE);
        uidev.id.bustype = BUS_USB;
        uidev.id.vendor = 0x1234;
        uidev.id.product = 0x5678;
        uidev.id.version = 1;

        ioctl(simulator->scroll_fd, UI_SET_EVBIT, EV_REL);
        ioctl(simulator->scroll_fd, UI_SET_RELBIT, REL_WHEEL);
        ioctl(simulator->scroll_fd, UI_SET_RELBIT, REL_HWHEEL);
        ioctl(simulator->scroll_fd, UI_SET_RELBIT, REL_WHEEL_HI_RES);
        ioctl(simulator->scroll_fd, UI_SET_RELBIT, REL_HWHEEL_HI_RES);
        ioctl(simulator->scroll_fd, UI_SET_EVBIT, EV_SYN);

        if (write(simulator->scroll_fd, &uidev, sizeof(uidev)) == -1) {
            perror("Error setting up device");
            return -1;
        }

        if (ioctl(simulator->scroll_fd, UI_DEV_CREATE) == -1) {
            perror("Error creating uinput device");
            return -1;
        }
    }

    {
        struct uinput_user_dev uidev = {0};

        strncpy(uidev.name, "Tenebra Pen Device", UINPUT_MAX_NAME_SIZE);
        uidev.id.bustype = BUS_USB;
        uidev.id.vendor = 0x1234;
        uidev.id.product = 0x5678;
        uidev.id.version = 1;

        ioctl(simulator->pen_fd, UI_SET_EVBIT, EV_SYN);
        ioctl(simulator->pen_fd, UI_SET_EVBIT, EV_ABS);
        ioctl(simulator->pen_fd, UI_SET_ABSBIT, ABS_X);
        ioctl(simulator->pen_fd, UI_SET_ABSBIT, ABS_Y);
        ioctl(simulator->pen_fd, UI_SET_ABSBIT, ABS_PRESSURE);
        ioctl(simulator->pen_fd, UI_SET_ABSBIT, ABS_TILT_X);
        ioctl(simulator->pen_fd, UI_SET_ABSBIT, ABS_TILT_Y);
        ioctl(simulator->pen_fd, UI_SET_EVBIT, EV_KEY);
        ioctl(simulator->pen_fd, UI_SET_KEYBIT, BTN_TOOL_PEN);
        ioctl(simulator->pen_fd, UI_SET_PROPBIT, INPUT_PROP_POINTER);
        ioctl(simulator->pen_fd, UI_SET_PROPBIT, INPUT_PROP_DIRECT);

        uidev.absmin[ABS_X] = 0;
        uidev.absmax[ABS_X] = 2000;
        uidev.absmin[ABS_Y] = 0;
        uidev.absmax[ABS_Y] = 2000;

        uidev.absmin[ABS_PRESSURE] = 0;
        uidev.absmax[ABS_PRESSURE] = 1000;

        uidev.absmin[ABS_TILT_X] = -90;
        uidev.absmax[ABS_TILT_X] = 90;
        uidev.absmin[ABS_TILT_Y] = -90;
        uidev.absmax[ABS_TILT_Y] = 90;

        if (write(simulator->pen_fd, &uidev, sizeof(uidev)) == -1) {
            perror("Error setting up device");
            return -1;
        }

        if (ioctl(simulator->pen_fd, UI_DEV_CREATE) == -1) {
            perror("Error creating uinput device");
            return -1;
        }

        emit_event(simulator->pen_fd, EV_KEY, BTN_TOOL_PEN, 1);
    }

    return 0;
}

MultiTouchSimulator* create_simulator() {
    MultiTouchSimulator* simulator = (MultiTouchSimulator*) malloc(sizeof(MultiTouchSimulator));

    simulator->touch_fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
    if (simulator->touch_fd == -1) {
        perror("Error opening /dev/uinput");
        free(simulator);
        return NULL;
    }
    simulator->scroll_fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
    if (simulator->scroll_fd == -1) {
        perror("Error opening /dev/uinput");
        close(simulator->touch_fd);
        free(simulator);
        return NULL;
    }
    simulator->pen_fd = open("/dev/uinput", O_WRONLY | O_NONBLOCK);
    if (simulator->pen_fd == -1) {
        perror("Error opening /dev/uinput");
        close(simulator->touch_fd);
        close(simulator->scroll_fd);
        free(simulator);
        return NULL;
    }

    simulator->wheel_x = 0;
    simulator->wheel_y = 0;

    if (setup_devices(simulator)) {
        close(simulator->touch_fd);
        close(simulator->scroll_fd);
        close(simulator->pen_fd);
        free(simulator);
        return NULL;
    }

    return simulator;
}

void destroy_simulator(MultiTouchSimulator* simulator) {
    if (simulator) {
        ioctl(simulator->touch_fd, UI_DEV_DESTROY);
        ioctl(simulator->scroll_fd, UI_DEV_DESTROY);
        ioctl(simulator->pen_fd, UI_DEV_DESTROY);
        close(simulator->touch_fd);
        close(simulator->scroll_fd);
        close(simulator->pen_fd);
    }
    free(simulator);
}

void touch_down(MultiTouchSimulator* simulator, int slot, int x, int y, int tracking_id) {
    emit_event(simulator->touch_fd, EV_ABS, ABS_MT_SLOT, slot);
    emit_event(simulator->touch_fd, EV_ABS, ABS_MT_TRACKING_ID, tracking_id);
    emit_event(simulator->touch_fd, EV_ABS, ABS_MT_POSITION_X, x);
    emit_event(simulator->touch_fd, EV_ABS, ABS_MT_POSITION_Y, y);
    emit_event(simulator->touch_fd, EV_SYN, SYN_REPORT, 0);
}

void touch_move(MultiTouchSimulator* simulator, int slot, int x, int y) {
    emit_event(simulator->touch_fd, EV_ABS, ABS_MT_SLOT, slot);
    emit_event(simulator->touch_fd, EV_ABS, ABS_MT_POSITION_X, x);
    emit_event(simulator->touch_fd, EV_ABS, ABS_MT_POSITION_Y, y);
    emit_event(simulator->touch_fd, EV_SYN, SYN_REPORT, 0);
}

void touch_up(MultiTouchSimulator* simulator, int slot) {
    emit_event(simulator->touch_fd, EV_ABS, ABS_MT_SLOT, slot);
    emit_event(simulator->touch_fd, EV_ABS, ABS_MT_TRACKING_ID, -1);
    emit_event(simulator->touch_fd, EV_SYN, SYN_REPORT, 0);
}

void scroll_vertically(MultiTouchSimulator* simulator, int value) {
    if (value) {
        simulator->wheel_y += value;
        emit_event(simulator->scroll_fd, EV_REL, REL_WHEEL_HI_RES, -value);
        if (abs(simulator->wheel_y) >= 120) {
            emit_event(simulator->scroll_fd, EV_REL, REL_WHEEL, -simulator->wheel_y / 120);
            simulator->wheel_y = simulator->wheel_y % 120;
        }
        emit_event(simulator->scroll_fd, EV_SYN, SYN_REPORT, 0);
    }
}

void scroll_horizontally(MultiTouchSimulator* simulator, int value) {
    if (value) {
        simulator->wheel_x += value;
        emit_event(simulator->scroll_fd, EV_REL, REL_HWHEEL_HI_RES, value);
        if (abs(simulator->wheel_x) >= 120) {
            emit_event(simulator->scroll_fd, EV_REL, REL_HWHEEL, simulator->wheel_x / 120);
            simulator->wheel_x = simulator->wheel_x % 120;
        }
        emit_event(simulator->scroll_fd, EV_SYN, SYN_REPORT, 0);
    }
}

void pen(MultiTouchSimulator* simulator, int x, int y, double pressure, int tilt_x, int tilt_y) {
    emit_event(simulator->pen_fd, EV_ABS, ABS_X, x);
    emit_event(simulator->pen_fd, EV_ABS, ABS_Y, y);
    emit_event(simulator->pen_fd, EV_ABS, ABS_PRESSURE, pressure * 1000);
    emit_event(simulator->pen_fd, EV_ABS, ABS_TILT_X, tilt_x);
    emit_event(simulator->pen_fd, EV_ABS, ABS_TILT_Y, tilt_y);
    emit_event(simulator->pen_fd, EV_SYN, SYN_REPORT, 0);
}
