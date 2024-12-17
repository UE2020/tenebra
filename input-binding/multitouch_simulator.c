#include <fcntl.h>
#include <linux/uinput.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

typedef struct {
    int touch_fd;
    int scroll_fd;
} MultiTouchSimulator;

int setup_devices(MultiTouchSimulator* simulator) {
    {
        struct uinput_user_dev uidev = {0};

        strncpy(uidev.name, "Tenebra Multi-Touch Device", UINPUT_MAX_NAME_SIZE);
        uidev.id.bustype = BUS_USB;
        uidev.id.vendor = 0x1234;
        uidev.id.product = 0x5678;
        uidev.id.version = 1;

        ioctl(simulator->touch_fd, UI_SET_EVBIT, EV_ABS);
        ioctl(simulator->touch_fd, UI_SET_ABSBIT, ABS_MT_SLOT);
        ioctl(simulator->touch_fd, UI_SET_ABSBIT, ABS_MT_POSITION_X);
        ioctl(simulator->touch_fd, UI_SET_ABSBIT, ABS_MT_POSITION_Y);
        ioctl(simulator->touch_fd, UI_SET_ABSBIT, ABS_MT_TRACKING_ID);

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

        ioctl(simulator->touch_fd, UI_SET_EVBIT, EV_SYN);
        ioctl(simulator->touch_fd, UI_SET_PROPBIT, INPUT_PROP_DIRECT);

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

    return 0;
}

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

    if (setup_devices(simulator)) {
        close(simulator->touch_fd);
        close(simulator->scroll_fd);
        free(simulator);
        return NULL;
    }

    return simulator;
}

void destroy_simulator(MultiTouchSimulator* simulator) {
    if (simulator) {
        ioctl(simulator->touch_fd, UI_DEV_DESTROY);
        close(simulator->touch_fd);
    }
    free(simulator);
}

void scroll_vertically(MultiTouchSimulator* simulator, int value) {
    if (value) {
        emit_event(simulator->scroll_fd, EV_REL, REL_WHEEL_HI_RES, -value);
        emit_event(simulator->scroll_fd, EV_SYN, SYN_REPORT, 0);
    }
}

void scroll_horizontally(MultiTouchSimulator* simulator, int value) {
    if (value) {
        emit_event(simulator->scroll_fd, EV_REL, REL_HWHEEL_HI_RES, value);
        emit_event(simulator->scroll_fd, EV_SYN, SYN_REPORT, 0);
    }
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
