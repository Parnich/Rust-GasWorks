use eframe::egui;
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const R: f64 = 8.314; // Универсальная газовая постоянная

#[derive(Clone, Debug, PartialEq)]
struct Substance {
    name: String,
    is_gas: bool,
    molar_mass: f64,    // кг/моль
    boiling_point: f64, // Кельвин
    density: f64,       // кг/м^3
    heat_capacity: f64, // Дж/(кг*К)
}

struct Particle {
    pos: egui::Pos2,
    dir: egui::Vec2,
    substance_idx: usize,
}

#[derive(Clone)]
struct SpawnedEntity {
    substance_idx: usize,
    moles: f64,
}

struct GasWorksApp {
    // Термодинамика системы
    temperature: f64,
    volume: f64,
    ambient_pressure: f64,

    // База данных и состояние
    database: Vec<Substance>,
    entities: Vec<SpawnedEntity>,
    particles: Vec<Particle>,

    // Инструменты спавна
    spawn_amount_moles: f64,
    selected_substance: usize,

    // Триггер искры для поджигания реакций вручную
    spark_trigger: bool,
}

impl Default for GasWorksApp {
    fn default() -> Self {
        Self {
            temperature: 298.15, // 25°C
            volume: 1.0,
            ambient_pressure: 101325.0,
            database: build_substance_database(),
            entities: Vec::new(),
            particles: Vec::new(),
            spawn_amount_moles: 2.0,
            selected_substance: 0, // По умолчанию Водород
            spark_trigger: false,
        }
    }
}

impl eframe::App for GasWorksApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.stable_dt).min(0.03); // Ограничиваем шаг физики

        // Вызов физического и химического движков
        self.update_physics(dt);
        self.check_chemical_reactions();

        // Отрисовка интерфейса
        self.window_settings(ctx);
        self.window_formulas_and_stats(ctx);
        self.window_spawner(ctx);
        self.window_constants(ctx);
        self.window_viewport(ctx);

        ctx.request_repaint();
    }
}

impl GasWorksApp {
    fn update_physics(&mut self, dt: f32) {
        let mut rng = rand::thread_rng();

        for particle in &mut self.particles {
            let sub = &self.database[particle.substance_idx];
            let is_currently_gas = sub.is_gas || self.temperature >= sub.boiling_point;

            if is_currently_gas {
                // Скорость по МКТ: v = sqrt(3RT/M)
                let v_rms = ((3.0 * R * self.temperature) / sub.molar_mass).sqrt();
                let speed_scale = (v_rms * 0.0005) as f32; // Коэффициент адаптации под пиксели

                particle.pos += particle.dir * speed_scale * dt;

                if particle.pos.x <= 0.0 { particle.pos.x = 0.0; particle.dir.x *= -1.0; }
                if particle.pos.x >= 1.0 { particle.pos.x = 1.0; particle.dir.x *= -1.0; }
                if particle.pos.y <= 0.0 { particle.pos.y = 0.0; particle.dir.y *= -1.0; }
                if particle.pos.y >= 1.0 { particle.pos.y = 1.0; particle.dir.y *= -1.0; }
            } else {
                // Жидкость стекает вниз
                particle.pos.y += 0.3 * dt;
                if particle.pos.y > 0.96 {
                    particle.pos.y = rng.gen_range(0.94..=0.98);
                }
                particle.pos.x += particle.dir.x * 0.05 * dt;
                if particle.pos.x <= 0.0 || particle.pos.x >= 1.0 { particle.dir.x *= -1.0; }
            }
        }
    }

    // Химический движок реального времени
    fn check_chemical_reactions(&mut self) {
        // Индексы веществ в нашей БД: 0 - Водород (H2), 3 - Кислород (O2), 20 - Вода (H2O)
        let h2_idx = 0;
        let o2_idx = 3;
        let h2o_idx = 20;

        let mut h2_moles = 0.0;
        let mut o2_moles = 0.0;

        for entity in &self.entities {
            if entity.substance_idx == h2_idx { h2_moles += entity.moles; }
            if entity.substance_idx == o2_idx { o2_moles += entity.moles; }
        }

        // Условие реакции: Температура > 773К (500°C) ИЛИ нажата кнопка "Искры"
        let thermal_activation = self.temperature >= 773.15;
        if (thermal_activation || self.spark_trigger) && h2_moles > 0.01 && o2_moles > 0.01 {

            // Расчет лимитирующего реагента по стехиометрии (2 H2 + 1 O2 -> 2 H2O)
            let limit_by_h2 = h2_moles / 2.0;
            let reaction_cycles = limit_by_h2.min(o2_moles).min(0.1); // ограничиваем скорость реакции за тик

            let h2_consumed = reaction_cycles * 2.0;
            let o2_consumed = reaction_cycles;
            let h2o_produced = reaction_cycles * 2.0;

            // Обновляем моли в термодинамической системе
            self.consume_substance(h2_idx, h2_consumed);
            self.consume_substance(o2_idx, o2_consumed);
            self.add_substance_moles(h2o_idx, h2o_produced);

            // Реакция горения экзотермическая! Выделяется ~241.8 кДж энергии на моль H2O
            let energy_released = h2o_produced * 241800.0;

            // Нагрев системы: dT = Q / (m * c). Упрощенно повышаем температуру среды:
            self.temperature += energy_released * 0.005;

            // Обновляем графические частицы: превращаем H2 и O2 в H2O
            self.convert_particles(h2_idx, o2_idx, h2o_idx, h2o_produced);
        }
        self.spark_trigger = false; // Сбрасываем триггер искры
    }

    fn consume_substance(&mut self, idx: usize, amount: f64) {
        if let Some(entity) = self.entities.iter_mut().find(|e| e.substance_idx == idx) {
            entity.moles -= amount;
            if entity.moles < 0.0 { entity.moles = 0.0; }
        }
        self.entities.retain(|e| e.moles > 0.01);
    }

    fn add_substance_moles(&mut self, idx: usize, amount: f64) {
        if let Some(entity) = self.entities.iter_mut().find(|e| e.substance_idx == idx) {
            entity.moles += amount;
        } else {
            self.entities.push(SpawnedEntity { substance_idx: idx, moles: amount });
        }
    }

    fn convert_particles(&mut self, r1: usize, r2: usize, product: usize, moles_produced: f64) {
        let mut rng = rand::thread_rng();
        let particles_to_spawn = (moles_produced * 25.0) as usize;

        // Удаляем часть старых частиц реагентов
        let mut removed = 0;
        self.particles.retain(|p| {
            if (p.substance_idx == r1 || p.substance_idx == r2) && removed < particles_to_spawn {
                removed += 1;
                false
            } else {
                true
            }
        });

        // Спавним новые частицы продукта реакции
        for _ in 0..particles_to_spawn {
            let angle = rng.gen_range(0.0..std::f32::consts::TAU);
            self.particles.push(Particle {
                pos: egui::pos2(rng.gen_range(0.2..0.8), rng.gen_range(0.2..0.8)),
                dir: egui::vec2(angle.cos(), angle.sin()),
                substance_idx: product,
            });
        }
    }

    // Окно 5: Контейнер и СИСТЕМА ПРЕДУПРЕЖДЕНИЙ
    fn window_viewport(&mut self, ctx: &egui::Context) {
        egui::Window::new("5. Камера симуляции").show(ctx, |ui| {

            // Расчет текущего давления для панели алармов
            let mut total_moles_gas = 0.0;
            for entity in &self.entities {
                if self.database[entity.substance_idx].is_gas || self.temperature >= self.database[entity.substance_idx].boiling_point {
                    total_moles_gas += entity.moles;
                }
            }
            let current_pressure = (total_moles_gas * R * self.temperature) / self.volume;

            // --- БЛОК АТМОСФЕРНЫХ ПРЕДУПРЕЖДЕНИЙ ---
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("ДАТЧИКИ БЕЗОПАСНОСТИ:");
                    if current_pressure > 400000.0 {
                        ui.colored_label(egui::Color32::RED, "⚠️ КРИТИЧЕСКОЕ ДАВЛЕНИЕ! ОПАСНОСТЬ ВЗРЫВА!");
                    } else if current_pressure > 200000.0 {
                        ui.colored_label(egui::Color32::GOLD, "⚠ Повышенное давление в камере");
                    } else if current_pressure < 5000.0 && !self.entities.is_empty() {
                        ui.colored_label(egui::Color32::LIGHT_BLUE, "❄ Низкое давление / Вакуум");
                    }

                    if self.temperature > 1500.0 {
                        ui.colored_label(egui::Color32::RED, "🔥 РАСПЛАВЛЕНИЕ СТЕНКА СРЕДЫ!");
                    } else if self.temperature > 773.15 {
                        ui.colored_label(egui::Color32::LIGHT_RED, "✴ Среда раскалена (Самовоспламенение газов)");
                    }
                });
            });
            ui.add_space(5.0);

            // Отрисовка графического окна контейнера
            let (response, painter) = ui.allocate_painter(egui::vec2(450.0, 400.0), egui::Sense::hover());
            let rect = response.rect;

            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(10, 10, 15));
            let stroke_color = if current_pressure > 400000.0 { egui::Color32::RED } else { egui::Color32::GRAY };
            painter.rect_stroke(rect, 4.0, egui::Stroke::new(3.0, stroke_color));

            for particle in &self.particles {
                let screen_x = rect.left() + particle.pos.x * rect.width();
                let screen_y = rect.top() + particle.pos.y * rect.height();
                let color = get_substance_color(&self.database[particle.substance_idx].name);

                painter.circle_filled(egui::pos2(screen_x, screen_y), 3.5, color);
            }
        });
    }

    fn window_settings(&mut self, ctx: &egui::Context) {
        egui::Window::new("1. Настройки системы").show(ctx, |ui| {
            ui.add(egui::Slider::new(&mut self.temperature, 1.0..=3500.0).text("Температура (K)"));
            ui.add(egui::Slider::new(&mut self.volume, 0.1..=20.0).text("Объем (м^3)"));
            ui.add(egui::Slider::new(&mut self.ambient_pressure, 0.0..=1000000.0).text("Атм. Давление (Pa)"));

            ui.separator();
            ui.heading("Твики и быстрое управление:");
            ui.horizontal(|ui| {
                if ui.button("❄ Охладить до 0°C").clicked() { self.temperature = 273.15; }
                if ui.button("💥 Дать искру (Поджег)").clicked() { self.spark_trigger = true; }
            });
        });
    }

    fn window_formulas_and_stats(&mut self, ctx: &egui::Context) {
        egui::Window::new("2. Характеристики и формулы").show(ctx, |ui| {
            ui.heading("Химическая активность:");
            ui.colored_label(egui::Color32::GREEN, "Доступна реакция: 2 H₂ + O₂ -> 2 H₂O + Энергия");
            ui.label("Порог автоподжига: T >= 773.15 K (500°C)");
            ui.separator();

            let mut total_moles_gas = 0.0;
            for entity in &self.entities {
                let is_gas = self.database[entity.substance_idx].is_gas || self.temperature >= self.database[entity.substance_idx].boiling_point;
                let state_str = if is_gas { "Газ" } else { "Жидк" };
                ui.label(format!("• {}: {:.2} моль [{}]", self.database[entity.substance_idx].name, entity.moles, state_str));
                if is_gas { total_moles_gas += entity.moles; }
            }

            let p_total = (total_moles_gas * R * self.temperature) / self.volume;
            ui.separator();
            ui.label(format!("Суммарное давление: {:.2} Pa", p_total));
            ui.label(format!("Общее число молекулярных групп: {}", self.particles.len()));
        });
    }

    fn window_spawner(&mut self, ctx: &egui::Context) {
        egui::Window::new("3. Спавн веществ").show(ctx, |ui| {
            ui.add(egui::Slider::new(&mut self.spawn_amount_moles, 0.1..=15.0).text("Количество (Моли)"));

            egui::ComboBox::from_label("Выбор")
                .selected_text(&self.database[self.selected_substance].name)
                .show_ui(ui, |ui| {
                    for (i, sub) in self.database.iter().enumerate() {
                        ui.selectable_value(&mut self.selected_substance, i, &sub.name);
                    }
                });

            if ui.button("Заспавнить").clicked() {
                self.add_substance_moles(self.selected_substance, self.spawn_amount_moles);

                let mut rng = rand::thread_rng();
                let count = (self.spawn_amount_moles * 25.0) as usize;
                for _ in 0..count {
                    let angle = rng.gen_range(0.0..std::f32::consts::TAU);
                    self.particles.push(Particle {
                        pos: egui::pos2(rng.gen_range(0.1..0.9), rng.gen_range(0.1..0.9)),
                        dir: egui::vec2(angle.cos(), angle.sin()),
                        substance_idx: self.selected_substance,
                    });
                }
            }

            ui.separator();
            ui.heading("Инструменты очистки поля:");
            ui.horizontal(|ui| {
                if ui.button("🧹 Очистить ВСЁ").clicked() {
                    self.entities.clear();
                    self.particles.clear();
                }
                if ui.button("💧 Удалить жидкости").clicked() {
                    let db = &self.database;
                    let t = self.temperature;
                    self.particles.retain(|p| db[p.substance_idx].is_gas || t >= db[p.substance_idx].boiling_point);
                    self.entities.retain(|e| db[e.substance_idx].is_gas || t >= db[e.substance_idx].boiling_point);
                }
            });
        });
    }

    fn window_constants(&mut self, ctx: &egui::Context) {
        egui::Window::new("4. Константы веществ").scroll2([false, true]).show(ctx, |ui| {
            for sub in &self.database {
                ui.collapsing(&sub.name, |ui| {
                    ui.label(format!("Молярная масса: {} кг/моль", sub.molar_mass));
                    ui.label(format!("Точка кипения: {} K", sub.boiling_point));
                    ui.label(format!("Плотность: {} кг/м^3", sub.density));
                });
            }
        });
    }
}

fn get_substance_color(name: &str) -> egui::Color32 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();
    let r = (hash & 0xFF) as u8;
    let g = ((hash >> 8) & 0xFF) as u8;
    let b = ((hash >> 16) & 0xFF) as u8;
    egui::Color32::from_rgb(r.max(90), g.max(90), b.max(80))
}

fn build_substance_database() -> Vec<Substance> {
    vec![
        // === ГАЗЫ (20) ===
        Substance { name: "Водород (H2)".into(), is_gas: true, molar_mass: 0.002016, boiling_point: 20.28, density: 0.08988, heat_capacity: 14300.0 },
        Substance { name: "Гелий (He)".into(), is_gas: true, molar_mass: 0.004002, boiling_point: 4.22, density: 0.1786, heat_capacity: 5193.0 },
        Substance { name: "Азот (N2)".into(), is_gas: true, molar_mass: 0.02801, boiling_point: 77.36, density: 1.2506, heat_capacity: 1040.0 },
        Substance { name: "Кислород (O2)".into(), is_gas: true, molar_mass: 0.032, boiling_point: 90.20, density: 1.429, heat_capacity: 918.0 },
        Substance { name: "Фтор (F2)".into(), is_gas: true, molar_mass: 0.038, boiling_point: 85.03, density: 1.696, heat_capacity: 824.0 },
        Substance { name: "Неон (Ne)".into(), is_gas: true, molar_mass: 0.02018, boiling_point: 27.07, density: 0.9002, heat_capacity: 1030.0 },
        Substance { name: "Хлор (Cl2)".into(), is_gas: true, molar_mass: 0.0709, boiling_point: 239.11, density: 3.2, heat_capacity: 480.0 },
        Substance { name: "Аргон (Ar)".into(), is_gas: true, molar_mass: 0.03995, boiling_point: 87.30, density: 1.784, heat_capacity: 520.0 },
        Substance { name: "Криптон (Kr)".into(), is_gas: true, molar_mass: 0.08379, boiling_point: 119.93, density: 3.749, heat_capacity: 248.0 },
        Substance { name: "Ксенон (Xe)".into(), is_gas: true, molar_mass: 0.13129, boiling_point: 165.03, density: 5.894, heat_capacity: 158.0 },
        Substance { name: "Радон (Rn)".into(), is_gas: true, molar_mass: 0.222, boiling_point: 211.3, density: 9.73, heat_capacity: 94.0 },
        Substance { name: "Угарный газ (CO)".into(), is_gas: true, molar_mass: 0.02801, boiling_point: 81.65, density: 1.145, heat_capacity: 1040.0 },
        Substance { name: "Углекислый газ (CO2)".into(), is_gas: true, molar_mass: 0.04401, boiling_point: 194.65, density: 1.977, heat_capacity: 844.0 },
        Substance { name: "Метан (CH4)".into(), is_gas: true, molar_mass: 0.01604, boiling_point: 111.6, density: 0.717, heat_capacity: 2220.0 },
        Substance { name: "Этан (C2H6)".into(), is_gas: true, molar_mass: 0.03007, boiling_point: 184.5, density: 1.356, heat_capacity: 1760.0 },
        Substance { name: "Пропан (C3H8)".into(), is_gas: true, molar_mass: 0.0441, boiling_point: 231.1, density: 2.01, heat_capacity: 1670.0 },
        Substance { name: "Бутан (C4H10)".into(), is_gas: true, molar_mass: 0.05812, boiling_point: 272.2, density: 2.48, heat_capacity: 1670.0 },
        Substance { name: "Аммиак (NH3)".into(), is_gas: true, molar_mass: 0.01703, boiling_point: 239.8, density: 0.769, heat_capacity: 2190.0 },
        Substance { name: "Диоксид серы (SO2)".into(), is_gas: true, molar_mass: 0.06406, boiling_point: 263.1, density: 2.926, heat_capacity: 620.0 },
        Substance { name: "Диоксид азота (NO2)".into(), is_gas: true, molar_mass: 0.04601, boiling_point: 294.3, density: 1.88, heat_capacity: 810.0 },

        // === ЖИДКОСТИ (10) ===
        Substance { name: "Вода (H2O)".into(), is_gas: false, molar_mass: 0.01801, boiling_point: 373.15, density: 997.0, heat_capacity: 4184.0 },
        Substance { name: "Этанол (C2H5OH)".into(), is_gas: false, molar_mass: 0.04607, boiling_point: 351.39, density: 789.0, heat_capacity: 2440.0 },
        Substance { name: "Метанол (CH3OH)".into(), is_gas: false, molar_mass: 0.03204, boiling_point: 337.8, density: 792.0, heat_capacity: 2530.0 },
        Substance { name: "Изопропанол (C3H8O)".into(), is_gas: false, molar_mass: 0.0601, boiling_point: 355.6, density: 786.0, heat_capacity: 2680.0 },
        Substance { name: "Ацетон (C3H6O)".into(), is_gas: false, molar_mass: 0.05808, boiling_point: 329.2, density: 790.0, heat_capacity: 2150.0 },
        Substance { name: "Глицерин (C3H8O3)".into(), is_gas: false, molar_mass: 0.09209, boiling_point: 563.0, density: 1261.0, heat_capacity: 2400.0 },
        Substance { name: "Ртуть (Hg)".into(), is_gas: false, molar_mass: 0.20059, boiling_point: 629.88, density: 13534.0, heat_capacity: 140.0 },
        Substance { name: "Бром (Br2)".into(), is_gas: false, molar_mass: 0.1598, boiling_point: 331.9, density: 3102.8, heat_capacity: 470.0 },
        Substance { name: "Гексан (C6H14)".into(), is_gas: false, molar_mass: 0.08618, boiling_point: 341.8, density: 654.8, heat_capacity: 2260.0 },
        Substance { name: "Октан (C8H18)".into(), is_gas: false, molar_mass: 0.11423, boiling_point: 398.8, density: 703.0, heat_capacity: 2230.0 },
    ]
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1250.0, 850.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GasWorks - Химическая Песочница",
        options,
        Box::new(|_cc| Box::new(GasWorksApp::default())),
    )
}