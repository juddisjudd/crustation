import { api } from './client';

export type Trigger =
	| { type: 'interval'; every_seconds: number }
	| { type: 'cron'; expression: string }
	| { type: 'after'; schedule_id: string; delay_seconds: number };

export type WipeKind = 'map' | 'blueprints' | 'full';

export type Action =
	| { type: 'start' }
	| { type: 'stop'; warn_seconds: number }
	| { type: 'restart'; warn_seconds: number }
	| { type: 'command'; command: string }
	| { type: 'wipe'; kind: WipeKind; new_seed: boolean; warn_seconds: number }
	| { type: 'update'; warn_seconds: number; only_if_newer: boolean };

export interface Schedule {
	id: string;
	name: string;
	enabled: boolean;
	trigger: Trigger;
	action: Action;
	one_time: boolean;
	next_run_at: string | null;
	last_run_at: string | null;
	last_result: string | null;
}

export interface ScheduleInput {
	name: string;
	enabled: boolean;
	trigger: Trigger;
	action: Action;
	one_time: boolean;
}

const base = (serverId: string) => `/servers/${serverId}/schedules`;

export const listSchedules = (serverId: string) => api.get<Schedule[]>(base(serverId));

export const createSchedule = (serverId: string, input: ScheduleInput) =>
	api.post<Schedule>(base(serverId), input);

export const updateSchedule = (serverId: string, id: string, input: Partial<ScheduleInput>) =>
	api.patch<Schedule>(`${base(serverId)}/${id}`, input);

export const deleteSchedule = (serverId: string, id: string) =>
	api.delete(`${base(serverId)}/${id}`);

export const runSchedule = (serverId: string, id: string) =>
	api.post(`${base(serverId)}/${id}/run`);
