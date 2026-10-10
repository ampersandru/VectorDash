"""Pure scheduling policy: telemetry is supplied by the desktop controller."""
from datetime import datetime, timedelta

def idle_allowed(settings, metrics):
    idle = metrics.get('idle')
    if settings.get('check_user_idle',True) and idle is None:
        return False, 'User idle monitoring unavailable'
    if settings.get('check_user_idle',True) and idle < settings['idle_minutes']*60:
        return False, 'Waiting for keyboard and mouse to be idle'
    if settings['check_cpu']:
        cpu = metrics.get('cpu')
        if cpu is None:
            return False, 'CPU monitoring unavailable'
        if cpu > settings['cpu_limit']:
            return False, 'Waiting for CPU usage to fall'
    if settings['check_gpu']:
        gpu = metrics.get('gpu')
        if gpu is None:
            return False, 'GPU monitoring unavailable; disable GPU condition to proceed'
        if gpu > settings['gpu_limit']:
            return False, 'Waiting for GPU usage to fall'
    if settings['ac_only'] and metrics.get('on_ac') is not True:
        return False, 'Waiting for AC power'
    return True, 'Idle conditions met'


def latest_slot(settings, now):
    hour, minute = map(int,settings['schedule_time'].split(':'))
    for offset in range(8):
        date = now.date() - timedelta(days=offset)
        candidate = datetime.combine(date,datetime.min.time()).replace(hour=hour,minute=minute)
        if date.weekday() in settings['schedule_days'] and candidate <= now:
            return candidate
    return None


def schedule_due(settings, now, metrics):
    mode = settings['schedule_mode']
    if mode == 'Manual':
        return False, '', 'Automatic indexing is off'
    if mode == 'When idle':
        last = settings.get('last_success')
        if last and now < datetime.fromisoformat(last)+timedelta(minutes=settings['idle_interval']):
            return False, '', 'Waiting for the next idle indexing interval'
        allowed, reason = idle_allowed(settings,metrics)
        return allowed, now.isoformat(), reason
    slot = latest_slot(settings,now)
    if not slot:
        return False, '', 'Waiting for the selected day and time'
    # On first enabling, avoid running a schedule from yesterday immediately.
    enabled = settings.get('schedule_enabled_at')
    if enabled and slot < datetime.fromisoformat(enabled):
        return False, '', 'Waiting for the selected day and time'
    previous = settings.get('last_schedule_slot')
    if previous and slot <= datetime.fromisoformat(previous):
        return False, '', 'Waiting for the next scheduled time'
    if settings['ac_only'] and metrics.get('on_ac') is not True:
        return False, '', 'Waiting for AC power'
    if mode == 'Time + idle':
        allowed, reason = idle_allowed(settings,metrics)
        return allowed, slot.isoformat(), reason
    return True, slot.isoformat(), 'Scheduled indexing is due'


def next_run(settings, now):
    if settings['schedule_mode'] == 'Manual':
        return 'Manual only'
    if settings['schedule_mode'] == 'When idle':
        return 'When idle conditions are met'
    hour,minute=map(int,settings['schedule_time'].split(':'))
    for offset in range(8):
        candidate=(now+timedelta(days=offset)).replace(hour=hour,minute=minute,second=0,microsecond=0)
        if candidate.weekday() in settings['schedule_days'] and candidate>now:
            return candidate.strftime('%a %b %d, %I:%M %p')
    return 'No days selected'
