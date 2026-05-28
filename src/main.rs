use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const R: f64 = 8.314;

#[derive(Clone, Debug, PartialEq)]
struct Substance {
    name: String,
    molar_mass: f64,
    boiling_point: f64,
    density: f64,
    heat_capacity: f64,
    vdw_a: f64,
    vdw_b: f64,
    latent_heat: f64,
}

struct Reaction {
    reactants: Vec<(usize, f64)>,
    products: Vec<(usize, f64)>,
    activation_temp: f64,
    enthalpy_change: f64,
}

struct Particle {
    pos: egui::Pos2,
    dir: egui::Vec2,
    substance_idx: usize,
    is_gas: bool,
}

#[derive(Clone)]
struct SpawnedEntity {
    substance_idx: usize,
    gas_moles: f64,
    liquid_moles: f64,
}

struct GasWorksApp {
    temperature: f64,
    volume: f64,
    ambient_pressure: f64,
    ambient_temp: f64,
    heat_transfer_coeff: f64,
    max_pressure: f64,
    exploded: bool,
    valve_open: bool,
    spark_trigger: bool,

    database: Vec<Substance>,
    reactions: Vec<Reaction>,
    entities: Vec<SpawnedEntity>,
    particles: Vec<Particle>,

    spawn_amount_moles: f64,
    selected_substance: usize,

    time: f64,
    history_p: Vec<[f64; 2]>,
    history_t: Vec<[f64; 2]>,
}

impl Default for GasWorksApp {
    fn default() -> Self {
        Self {
            temperature: 298.15,
            volume: 2.0,
            ambient_pressure: 101325.0,
            ambient_temp: 298.15,
            heat_transfer_coeff: 3.0,
            max_pressure: 2_500_000.0,
            exploded: false,
            valve_open: false,
            spark_trigger: false,
            database: build_substance_database(),
            reactions: build_reactions(),
            entities: Vec::new(),
            particles: Vec::new(),
            spawn_amount_moles: 2.0,
            selected_substance: 0,
            time: 0.0,
            history_p: Vec::new(),
            history_t: Vec::new(),
        }
    }
}

impl eframe::App for GasWorksApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.stable_dt).min(0.03);

        if !self.exploded {
            self.time += dt as f64;
            self.update_thermodynamics(dt as f64);
            self.sync_particles();
            self.update_physics(dt);
            self.check_chemical_reactions();
            self.record_history();
        }

        self.window_settings(ctx);
        self.window_indicators(ctx);
        self.window_formulas_and_stats(ctx);
        self.window_graphs(ctx);
        self.window_spawner(ctx);
        self.window_viewport(ctx);
        self.window_substance_info(ctx);

        ctx.request_repaint();
    }
}

impl GasWorksApp {
    fn calculate_pressure(&self) -> f64 {
        let mut p_total = 0.0;
        for entity in &self.entities {
            let sub = &self.database[entity.substance_idx];
            let n = entity.gas_moles;
            if n > 0.0 {
                let vdw_term1 = (n * R * self.temperature) / (self.volume - n * sub.vdw_b).max(0.001);
                let vdw_term2 = (sub.vdw_a * n * n) / (self.volume * self.volume);
                p_total += (vdw_term1 - vdw_term2).max(0.0);
            }
        }
        p_total
    }

    fn update_thermodynamics(&mut self, dt: f64) {
        let mut total_heat_capacity = 0.0;
        for entity in &self.entities {
            let sub = &self.database[entity.substance_idx];
            let mass = (entity.gas_moles + entity.liquid_moles) * sub.molar_mass;
            total_heat_capacity += mass * sub.heat_capacity;
        }
        if total_heat_capacity < 1.0 { total_heat_capacity = 1.0; }

        let heat_exchange = self.heat_transfer_coeff * (self.ambient_temp - self.temperature) * dt;
        self.temperature += heat_exchange / total_heat_capacity;

        let conversion_rate = 0.8;
        for entity in &mut self.entities {
            let sub = &self.database[entity.substance_idx];
            if self.temperature > sub.boiling_point {
                let diff = self.temperature - sub.boiling_point;
                let convert = (diff * conversion_rate * dt).min(entity.liquid_moles);
                entity.liquid_moles -= convert;
                entity.gas_moles += convert;
            } else {
                let diff = sub.boiling_point - self.temperature;
                let convert = (diff * conversion_rate * dt).min(entity.gas_moles);
                entity.gas_moles -= convert;
                entity.liquid_moles += convert;
            }
        }

        if self.valve_open {
            let p_in = self.calculate_pressure();
            if p_in > self.ambient_pressure {
                let drop_ratio = 1.0 - (5.5 * dt);
                for entity in &mut self.entities {
                    entity.gas_moles = (entity.gas_moles * drop_ratio).max(0.0);
                }
                self.entities.retain(|e| (e.gas_moles + e.liquid_moles) > 0.001);
            }
        }

        let current_p = self.calculate_pressure();
        if current_p > self.max_pressure {
            self.exploded = true;
        }
    }

    fn sync_particles(&mut self) {
        let mut rng = rand::thread_rng();

        for (idx, _) in self.database.iter().enumerate() {
            let (gas_moles, liq_moles) = self.entities.iter()
                .find(|e| e.substance_idx == idx)
                .map(|e| (e.gas_moles, e.liquid_moles))
                .unwrap_or((0.0, 0.0));

            let target_gas = (gas_moles * 15.0) as usize;
            let target_liq = (liq_moles * 15.0) as usize;

            let mut p_gas = Vec::new();
            let mut p_liq = Vec::new();
            for (p_i, p) in self.particles.iter().enumerate() {
                if p.substance_idx == idx {
                    if p.is_gas { p_gas.push(p_i); } else { p_liq.push(p_i); }
                }
            }

            let mut curr_gas = p_gas.len();
            let mut curr_liq = p_liq.len();

            while curr_gas < target_gas && curr_liq > target_liq {
                if let Some(&p_i) = p_liq.last() {
                    self.particles[p_i].is_gas = true;
                    self.particles[p_i].dir = egui::vec2(rng.gen_range(-1.0..1.0), rng.gen_range(-1.0..0.0)).normalized();
                    p_gas.push(p_i);
                    p_liq.pop();
                    curr_gas += 1;
                    curr_liq -= 1;
                } else { break; }
            }

            while curr_liq < target_liq && curr_gas > target_gas {
                if let Some(&p_i) = p_gas.last() {
                    self.particles[p_i].is_gas = false;
                    self.particles[p_i].dir = egui::vec2(rng.gen_range(-1.0..1.0), 0.0);
                    p_liq.push(p_i);
                    p_gas.pop();
                    curr_liq += 1;
                    curr_gas -= 1;
                } else { break; }
            }

            while curr_gas < target_gas {
                let angle = rng.gen_range(0.0..std::f32::consts::TAU);
                self.particles.push(Particle {
                    pos: egui::pos2(rng.gen_range(0.1..0.9), rng.gen_range(0.1..0.5)),
                    dir: egui::vec2(angle.cos(), angle.sin()),
                    substance_idx: idx,
                    is_gas: true,
                });
                curr_gas += 1;
            }

            while curr_liq < target_liq {
                self.particles.push(Particle {
                    pos: egui::pos2(rng.gen_range(0.1..0.9), rng.gen_range(0.8..0.9)),
                    dir: egui::vec2(rng.gen_range(-1.0..1.0), 0.0),
                    substance_idx: idx,
                    is_gas: false,
                });
                curr_liq += 1;
            }
        }

        for idx in 0..self.database.len() {
            let (gas_moles, liq_moles) = self.entities.iter()
                .find(|e| e.substance_idx == idx)
                .map(|e| (e.gas_moles, e.liquid_moles))
                .unwrap_or((0.0, 0.0));
            let target_gas = (gas_moles * 15.0) as usize;
            let target_liq = (liq_moles * 15.0) as usize;

            let mut c_g = 0;
            let mut c_l = 0;
            self.particles.retain(|p| {
                if p.substance_idx == idx {
                    if p.is_gas {
                        if c_g < target_gas { c_g += 1; true } else { false }
                    } else {
                        if c_l < target_liq { c_l += 1; true } else { false }
                    }
                } else {
                    true
                }
            });
        }
    }

    fn update_physics(&mut self, dt: f32) {
        let mut rng = rand::thread_rng();

        let mut active_liquids: Vec<(usize, f64, f64)> = self.entities.iter()
            .filter(|e| e.liquid_moles > 0.001)
            .map(|e| (e.substance_idx, e.liquid_moles, self.database[e.substance_idx].density))
            .collect();

        active_liquids.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());

        let mut layers = std::collections::HashMap::new();
        let mut current_y = 1.0;

        for (idx, moles, _) in active_liquids {
            let height = (moles * 0.004).min(0.2) as f32;
            layers.insert(idx, (current_y - height, current_y));
            current_y -= height;
        }

        for particle in &mut self.particles {
            let sub = &self.database[particle.substance_idx];

            if particle.is_gas {
                let v_rms = ((3.0 * R * self.temperature) / sub.molar_mass).sqrt();
                let speed_scale = (v_rms * 0.00015) as f32;
                particle.pos += particle.dir * speed_scale * dt;

                if particle.pos.x <= 0.0 { particle.pos.x = 0.0; particle.dir.x *= -1.0; }
                if particle.pos.x >= 1.0 { particle.pos.x = 1.0; particle.dir.x *= -1.0; }
                if particle.pos.y <= 0.0 { particle.pos.y = 0.0; particle.dir.y *= -1.0; }
                if particle.pos.y >= 1.0 { particle.pos.y = 1.0; particle.dir.y *= -1.0; }
            } else {
                if let Some(&(top, bottom)) = layers.get(&particle.substance_idx) {
                    let target_y = rng.gen_range(top..bottom);
                    let dy = target_y - particle.pos.y;

                    particle.pos.y += dy * 3.0 * dt;
                    particle.pos.x += particle.dir.x * 0.08 * dt;

                    if particle.pos.x <= 0.0 { particle.pos.x = 0.0; particle.dir.x *= -1.0; }
                    if particle.pos.x >= 1.0 { particle.pos.x = 1.0; particle.dir.x *= -1.0; }
                }
            }
        }
    }

    fn check_chemical_reactions(&mut self) {
        let mut to_consume = Vec::new();
        let mut to_produce = Vec::new();
        let mut total_heat_change = 0.0;
        let mut reaction_triggered = false;

        for reaction in &self.reactions {
            let thermal_activation = self.temperature >= reaction.activation_temp;

            if thermal_activation || self.spark_trigger {
                let mut can_react = true;
                let mut limiting_factor: f64 = 999999.0;

                for (idx, coef) in &reaction.reactants {
                    let mut available = 0.0;
                    for e in &self.entities {
                        if e.substance_idx == *idx { available += e.gas_moles + e.liquid_moles; }
                    }
                    if available < *coef * 0.01 {
                        can_react = false;
                        break;
                    }
                    let cycles = available / coef;
                    if cycles < limiting_factor { limiting_factor = cycles; }
                }

                if can_react {
                    let cycles_this_tick = limiting_factor.min(0.06);

                    for (idx, coef) in &reaction.reactants {
                        to_consume.push((*idx, cycles_this_tick * coef));
                    }
                    for (idx, coef) in &reaction.products {
                        to_produce.push((*idx, cycles_this_tick * coef));
                    }

                    total_heat_change += reaction.enthalpy_change * cycles_this_tick;
                    reaction_triggered = true;
                }
            }
        }

        for (idx, amount) in to_consume {
            self.consume_substance(idx, amount);
        }
        for (idx, amount) in to_produce {
            self.add_substance_moles(idx, amount);
        }

        let mut total_heat_capacity = 0.0;
        for entity in &self.entities {
            let sub = &self.database[entity.substance_idx];
            total_heat_capacity += (entity.gas_moles + entity.liquid_moles) * sub.molar_mass * sub.heat_capacity;
        }
        if total_heat_capacity > 1.0 && total_heat_change != 0.0 {
            self.temperature += total_heat_change / total_heat_capacity;
        }

        if reaction_triggered {
            self.spark_trigger = false;
        }
    }

    fn consume_substance(&mut self, idx: usize, amount: f64) {
        if let Some(entity) = self.entities.iter_mut().find(|e| e.substance_idx == idx) {
            let mut remaining = amount;
            if entity.gas_moles > 0.0 {
                let take = remaining.min(entity.gas_moles);
                entity.gas_moles -= take;
                remaining -= take;
            }
            if remaining > 0.0 {
                entity.liquid_moles -= remaining;
                if entity.liquid_moles < 0.0 { entity.liquid_moles = 0.0; }
            }
        }
        self.entities.retain(|e| (e.gas_moles + e.liquid_moles) > 0.001);
    }

    fn add_substance_moles(&mut self, idx: usize, amount: f64) {
        let is_gas = self.temperature >= self.database[idx].boiling_point;
        if let Some(entity) = self.entities.iter_mut().find(|e| e.substance_idx == idx) {
            if is_gas { entity.gas_moles += amount; } else { entity.liquid_moles += amount; }
        } else {
            let mut e = SpawnedEntity { substance_idx: idx, gas_moles: 0.0, liquid_moles: 0.0 };
            if is_gas { e.gas_moles = amount; } else { e.liquid_moles = amount; }
            self.entities.push(e);
        }
    }

    fn record_history(&mut self) {
        self.history_p.push([self.time, self.calculate_pressure()]);
        self.history_t.push([self.time, self.temperature]);
        if self.history_p.len() > 400 { self.history_p.remove(0); }
        if self.history_t.len() > 400 { self.history_t.remove(0); }
    }

    fn window_settings(&mut self, ctx: &egui::Context) {
        egui::Window::new("1. Управление реактором").show(ctx, |ui| {
            if self.exploded {
                ui.colored_label(egui::Color32::RED, "КОНТЕЙНЕР РАЗРУШЕН. ПЕРЕЗАПУСТИТЕ ПРИЛОЖЕНИЕ.");
                return;
            }
            ui.add(egui::Slider::new(&mut self.volume, 0.1..=15.0).text("Объем камеры (м^3)"));
            ui.add(egui::Slider::new(&mut self.heat_transfer_coeff, 0.0..=40.0).text("Теплопроводность стенок"));
            ui.add(egui::Slider::new(&mut self.ambient_temp, 5.0..=1200.0).text("Внешняя темп. (K)"));

            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.valve_open, "Аварийный клапан сброса");
                if ui.button("💥 ДАТЬ ИСКРУ").clicked() {
                    self.spark_trigger = true;
                }
            });
        });
    }

    fn draw_lamp(ui: &mut egui::Ui, state: bool, active_color: egui::Color32, label: &str) {
        ui.horizontal(|ui| {
            let (rect, _response) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            let color = if state { active_color } else { egui::Color32::from_gray(50) };
            ui.painter().circle_filled(rect.center(), 6.5, color);
            ui.label(label);
        });
    }

    fn window_indicators(&mut self, ctx: &egui::Context) {
        egui::Window::new("2. Сигнальные лампы").show(ctx, |ui| {
            let p = self.calculate_pressure();
            let t = self.temperature;

            let warn_pressure = p > self.max_pressure * 0.75;
            let warn_vacuum = p < 4000.0 && !self.entities.is_empty();
            let warn_temp = t > 773.15;

            Self::draw_lamp(ui, warn_pressure, egui::Color32::RED, "КРИТИЧЕСКОЕ ДАВЛЕНИЕ");
            Self::draw_lamp(ui, warn_vacuum, egui::Color32::LIGHT_BLUE, "ВАКУУМ");
            Self::draw_lamp(ui, warn_temp, egui::Color32::from_rgb(255, 69, 0), "ТЕРМИЧЕСКАЯ АКТИВАЦИЯ");
            Self::draw_lamp(ui, self.valve_open, egui::Color32::YELLOW, "СБРОС МАССЫ");
            Self::draw_lamp(ui, self.exploded, egui::Color32::RED, "ПОВРЕЖДЕНИЕ КОРПУСА");
        });
    }

    fn window_formulas_and_stats(&mut self, ctx: &egui::Context) {
        egui::Window::new("3. Состояние компонентов").show(ctx, |ui| {
            ui.label(format!("Температура среды: {:.1} K", self.temperature));
            ui.label(format!("Давление Ван-дер-Ваальса: {:.0} Pa", self.calculate_pressure()));

            ui.separator();
            ui.heading("Содержимое:");
            for e in &self.entities {
                let sub = &self.database[e.substance_idx];
                ui.label(format!("• {}:", sub.name));
                if e.gas_moles > 0.001 { ui.label(format!("  - Газ: {:.2} моль", e.gas_moles)); }
                if e.liquid_moles > 0.001 { ui.label(format!("  - Жидкость: {:.2} моль", e.liquid_moles)); }
            }
        });
    }

    fn window_graphs(&mut self, ctx: &egui::Context) {
        egui::Window::new("4. Мониторинг").show(ctx, |ui| {
            let line_p = Line::new(PlotPoints::from_iter(self.history_p.clone())).color(egui::Color32::from_rgb(0, 191, 255));
            Plot::new("plot_p_complex").height(130.0).show(ui, |plot_ui| plot_ui.line(line_p));

            let line_t = Line::new(PlotPoints::from_iter(self.history_t.clone())).color(egui::Color32::from_rgb(255, 99, 71));
            Plot::new("plot_t_complex").height(130.0).show(ui, |plot_ui| plot_ui.line(line_t));
        });
    }

    fn window_spawner(&mut self, ctx: &egui::Context) {
        egui::Window::new("5. Шлюз").show(ctx, |ui| {
            ui.add(egui::Slider::new(&mut self.spawn_amount_moles, 0.2..=20.0).text("Количество (моль)"));

            egui::ComboBox::from_label("")
                .selected_text(&self.database[self.selected_substance].name)
                .show_ui(ui, |ui| {
                    for (i, sub) in self.database.iter().enumerate() {
                        ui.selectable_value(&mut self.selected_substance, i, &sub.name);
                    }
                });

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                if ui.button("📥 Закачать").clicked() {
                    self.add_substance_moles(self.selected_substance, self.spawn_amount_moles);
                }
                if ui.button("🧹 Очистка").clicked() {
                    self.entities.clear();
                    self.particles.clear();
                }
            });
        });
    }

    fn window_viewport(&mut self, ctx: &egui::Context) {
        egui::Window::new("6. Камера").show(ctx, |ui| {
            let (response, painter) = ui.allocate_painter(egui::vec2(450.0, 420.0), egui::Sense::hover());
            let rect = response.rect;

            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(10, 10, 12));

            if self.exploded {
                painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(60, 10, 10));
                painter.line_segment([rect.left_top(), rect.right_bottom()], egui::Stroke::new(6.0, egui::Color32::RED));
                painter.line_segment([rect.right_top(), rect.left_bottom()], egui::Stroke::new(6.0, egui::Color32::RED));
                return;
            }

            let border_color = if self.calculate_pressure() > self.max_pressure * 0.75 { egui::Color32::RED } else { egui::Color32::DARK_GRAY };
            painter.rect_stroke(rect, 0.0, egui::Stroke::new(3.0, border_color));

            for p in &self.particles {
                let sx = rect.left() + p.pos.x * rect.width();
                let sy = rect.top() + p.pos.y * rect.height();
                let color = get_substance_color(&self.database[p.substance_idx].name);
                painter.circle_filled(egui::pos2(sx, sy), 3.0, color);
            }
        });
    }

    fn window_substance_info(&mut self, ctx: &egui::Context) {
        egui::Window::new("7. Константы веществ").show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for sub in &self.database {
                    ui.collapsing(&sub.name, |ui| {
                        ui.label(format!("M: {:.4} кг/моль", sub.molar_mass));
                        ui.label(format!("T_boil: {:.2} K", sub.boiling_point));
                        ui.label(format!("ρ_liq: {:.1} кг/м³", sub.density));
                        ui.label(format!("C: {:.1} Дж/(кг·K)", sub.heat_capacity));
                        ui.label(format!("vdw_a: {:.4} Па·м⁶/моль²", sub.vdw_a));
                        ui.label(format!("vdw_b: {:.6} м³/моль", sub.vdw_b));
                        ui.label(format!("L: {:.1} Дж/моль", sub.latent_heat));
                    });
                }
            });
        });
    }
}

fn get_substance_color(name: &str) -> egui::Color32 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();
    egui::Color32::from_rgb((hash & 0xFF).max(110) as u8, ((hash >> 8) & 0xFF).max(110) as u8, ((hash >> 16) & 0xFF).max(110) as u8)
}

fn build_substance_database() -> Vec<Substance> {
    vec![
        Substance { name: "Водород (H2)".into(), molar_mass: 0.002016, boiling_point: 20.28, density: 0.089, heat_capacity: 14300.0, vdw_a: 0.0248, vdw_b: 0.0000266, latent_heat: 900.0 },
        Substance { name: "Гелий (He)".into(), molar_mass: 0.004002, boiling_point: 4.22, density: 0.1786, heat_capacity: 5193.0, vdw_a: 0.0035, vdw_b: 0.0000237, latent_heat: 80.0 },
        Substance { name: "Азот (N2)".into(), molar_mass: 0.02801, boiling_point: 77.36, density: 808.0, heat_capacity: 1040.0, vdw_a: 0.1370, vdw_b: 0.0000386, latent_heat: 5560.0 },
        Substance { name: "Кислород (O2)".into(), molar_mass: 0.032, boiling_point: 90.20, density: 1141.0, heat_capacity: 918.0, vdw_a: 0.1378, vdw_b: 0.0000318, latent_heat: 6820.0 },
        Substance { name: "Фтор (F2)".into(), molar_mass: 0.038, boiling_point: 85.03, density: 1505.0, heat_capacity: 824.0, vdw_a: 0.1160, vdw_b: 0.0000290, latent_heat: 6500.0 },
        Substance { name: "Неон (Ne)".into(), molar_mass: 0.02018, boiling_point: 27.07, density: 1207.0, heat_capacity: 1030.0, vdw_a: 0.0210, vdw_b: 0.0000171, latent_heat: 440.0 },
        Substance { name: "Хлор (Cl2)".into(), molar_mass: 0.0709, boiling_point: 239.11, density: 1562.0, heat_capacity: 480.0, vdw_a: 0.6580, vdw_b: 0.0000562, latent_heat: 20400.0 },
        Substance { name: "Аргон (Ar)".into(), molar_mass: 0.03995, boiling_point: 87.30, density: 1395.0, heat_capacity: 520.0, vdw_a: 0.1350, vdw_b: 0.0000322, latent_heat: 6500.0 },
        Substance { name: "Криптон (Kr)".into(), molar_mass: 0.08379, boiling_point: 119.93, density: 2413.0, heat_capacity: 248.0, vdw_a: 0.2350, vdw_b: 0.0000398, latent_heat: 9080.0 },
        Substance { name: "Ксенон (Xe)".into(), molar_mass: 0.13129, boiling_point: 165.03, density: 2942.0, heat_capacity: 158.0, vdw_a: 0.4190, vdw_b: 0.0000511, latent_heat: 12640.0 },
        Substance { name: "Радон (Rn)".into(), molar_mass: 0.222, boiling_point: 211.3, density: 4400.0, heat_capacity: 94.0, vdw_a: 0.6300, vdw_b: 0.0000620, latent_heat: 18100.0 },
        Substance { name: "Угарный газ (CO)".into(), molar_mass: 0.02801, boiling_point: 81.65, density: 789.0, heat_capacity: 1040.0, vdw_a: 0.1470, vdw_b: 0.0000395, latent_heat: 6040.0 },
        Substance { name: "Углекислый газ (CO2)".into(), molar_mass: 0.04401, boiling_point: 194.65, density: 1562.0, heat_capacity: 844.0, vdw_a: 0.3640, vdw_b: 0.0000427, latent_heat: 25200.0 },
        Substance { name: "Метан (CH4)".into(), molar_mass: 0.01604, boiling_point: 111.6, density: 422.0, heat_capacity: 2220.0, vdw_a: 0.2283, vdw_b: 0.0000428, latent_heat: 8170.0 },
        Substance { name: "Этан (C2H6)".into(), molar_mass: 0.03007, boiling_point: 184.5, density: 544.0, heat_capacity: 1760.0, vdw_a: 0.5560, vdw_b: 0.0000638, latent_heat: 14700.0 },
        Substance { name: "Пропан (C3H8)".into(), molar_mass: 0.0441, boiling_point: 231.1, density: 582.0, heat_capacity: 1670.0, vdw_a: 0.9390, vdw_b: 0.0000905, latent_heat: 18800.0 },
        Substance { name: "Бутан (C4H10)".into(), molar_mass: 0.05812, boiling_point: 272.2, density: 600.0, heat_capacity: 1670.0, vdw_a: 1.4660, vdw_b: 0.0001226, latent_heat: 22400.0 },
        Substance { name: "Аммиак (NH3)".into(), molar_mass: 0.01703, boiling_point: 239.8, density: 681.0, heat_capacity: 2190.0, vdw_a: 0.4230, vdw_b: 0.0000373, latent_heat: 23350.0 },
        Substance { name: "Диоксид серы (SO2)".into(), molar_mass: 0.06406, boiling_point: 263.1, density: 1460.0, heat_capacity: 620.0, vdw_a: 0.6800, vdw_b: 0.0000564, latent_heat: 24900.0 },
        Substance { name: "Диоксид азота (NO2)".into(), molar_mass: 0.04601, boiling_point: 294.3, density: 1449.0, heat_capacity: 810.0, vdw_a: 0.5300, vdw_b: 0.0000440, latent_heat: 38000.0 },
        Substance { name: "Вода (H2O)".into(), molar_mass: 0.01801, boiling_point: 373.15, density: 997.0, heat_capacity: 4184.0, vdw_a: 0.5536, vdw_b: 0.0000305, latent_heat: 40650.0 },
        Substance { name: "Этанол (C2H5OH)".into(), molar_mass: 0.04607, boiling_point: 351.39, density: 789.0, heat_capacity: 2440.0, vdw_a: 1.2180, vdw_b: 0.0000841, latent_heat: 38600.0 },
        Substance { name: "Метанол (CH3OH)".into(), molar_mass: 0.03204, boiling_point: 337.8, density: 792.0, heat_capacity: 2530.0, vdw_a: 0.9650, vdw_b: 0.0000659, latent_heat: 35300.0 },
        Substance { name: "Изопропанол (C3H8O)".into(), molar_mass: 0.0601, boiling_point: 355.6, density: 786.0, heat_capacity: 2680.0, vdw_a: 1.5000, vdw_b: 0.0001100, latent_heat: 40000.0 },
        Substance { name: "Ацетон (C3H6O)".into(), molar_mass: 0.05808, boiling_point: 329.2, density: 790.0, heat_capacity: 2150.0, vdw_a: 1.6000, vdw_b: 0.0001100, latent_heat: 29100.0 },
        Substance { name: "Глицерин (C3H8O3)".into(), molar_mass: 0.09209, boiling_point: 563.0, density: 1261.0, heat_capacity: 2400.0, vdw_a: 2.0000, vdw_b: 0.0001400, latent_heat: 61000.0 },
        Substance { name: "Ртуть (Hg)".into(), molar_mass: 0.20059, boiling_point: 629.88, density: 13534.0, heat_capacity: 140.0, vdw_a: 0.8190, vdw_b: 0.0000170, latent_heat: 59100.0 },
        Substance { name: "Бром (Br2)".into(), molar_mass: 0.1598, boiling_point: 331.9, density: 3102.8, heat_capacity: 470.0, vdw_a: 1.0000, vdw_b: 0.0000700, latent_heat: 29900.0 },
        Substance { name: "Гексан (C6H14)".into(), molar_mass: 0.08618, boiling_point: 341.8, density: 654.8, heat_capacity: 2260.0, vdw_a: 2.4700, vdw_b: 0.0001700, latent_heat: 28900.0 },
        Substance { name: "Октан (C8H18)".into(), molar_mass: 0.11423, boiling_point: 398.8, density: 703.0, heat_capacity: 2230.0, vdw_a: 3.7600, vdw_b: 0.0002400, latent_heat: 34400.0 },
    ]
}

fn build_reactions() -> Vec<Reaction> {
    vec![
        Reaction {
            reactants: vec![(0, 2.0), (3, 1.0)],
            products: vec![(20, 2.0)],
            activation_temp: 773.15,
            enthalpy_change: 483600.0,
        },
        Reaction {
            reactants: vec![(13, 1.0), (3, 2.0)],
            products: vec![(12, 1.0), (20, 2.0)],
            activation_temp: 873.15,
            enthalpy_change: 802000.0,
        },
        Reaction {
            reactants: vec![(2, 1.0), (0, 3.0)],
            products: vec![(17, 2.0)],
            activation_temp: 673.15,
            enthalpy_change: 92400.0,
        },
        Reaction {
            reactants: vec![(11, 2.0), (3, 1.0)],
            products: vec![(12, 2.0)],
            activation_temp: 880.0,
            enthalpy_change: 566000.0,
        },
        Reaction {
            reactants: vec![(14, 2.0), (3, 7.0)],
            products: vec![(12, 4.0), (20, 6.0)],
            activation_temp: 800.0,
            enthalpy_change: 3120000.0,
        },
        Reaction {
            reactants: vec![(17, 4.0), (3, 3.0)],
            products: vec![(2, 2.0), (20, 6.0)],
            activation_temp: 1073.15,
            enthalpy_change: 1267000.0,
        },
        Reaction {
            reactants: vec![(15, 1.0), (3, 5.0)],
            products: vec![(12, 3.0), (20, 4.0)],
            activation_temp: 743.15,
            enthalpy_change: 2220000.0,
        },
        Reaction {
            reactants: vec![(16, 2.0), (3, 13.0)],
            products: vec![(12, 8.0), (20, 10.0)],
            activation_temp: 678.15,
            enthalpy_change: 5756000.0,
        },
        Reaction {
            reactants: vec![(21, 1.0), (3, 3.0)],
            products: vec![(12, 2.0), (20, 3.0)],
            activation_temp: 638.15,
            enthalpy_change: 1367000.0,
        },
        Reaction {
            reactants: vec![(22, 2.0), (3, 3.0)],
            products: vec![(12, 2.0), (20, 4.0)],
            activation_temp: 738.15,
            enthalpy_change: 1452000.0,
        }
    ]
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1450.0, 920.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GasWorks Reactor Core",
        options,
        Box::new(|_cc| Box::new(GasWorksApp::default())),
    )
}