# What is it?
_Rust-GasWorks_ is a open-source fork of my other project, _GasWorks_.  
Unlike the original, _Rust-GasWorks_ is not just a library, but a mini simulation game that allows us to see what results we have achieved in modelling and optimising the physics of gases and liquids.
# How it works?
The simulator operates on a game tick, which recalculates the interrelated physical and chemical processes at every frame:
### 1. Thermodynamics and the equation of state
The pressure in the system is not simply proportional to the amount of substance; it is calculated using **Jan Diederik van der Waals** equation for a mixture of real gases:

$$P_{total} = \sum_{i} \left( \frac{n_i R T}{V - n_i b_i} - \frac{a_i n_i^2}{V^2} \right)$$

Where $a$ and $b$ are substance-specific corrections for intermolecular attraction and the intrinsic volume of the molecules. Heat exchange with the environment through the reactor walls is also taken into account: the change in temperature depends on the total heat capacity of the mass inside the vessel.

### 2. Dynamics of phase transitions
The system constantly compares the current reactor temperature with the boiling points of each substance inside. 
* If the temperature exceeds the boiling point, the liquid evaporates at a specified rate.
* If it falls below the boiling point, the gas condenses into a liquid. At the same time, the graphics engine synchronously transfers particles from free ‘gaseous’ flight into liquid layers.
### 3. Chemical reaction kinetics
The simulator includes a database of reaction chains (e.g., ammonia synthesis, propane/butane combustion). A reaction is triggered in two cases:
1. **Thermal activation:** the temperature in the reactor has exceeded the activation energy threshold.
2. **Forced ignition:** manual ignition of the mixture by the user.
At every tick, the application identifies the rate-limiting reactant, calculates the rate of consumption and the change in moles, and adjusts the system temperature in accordance with the **reaction enthalpy** (exothermic processes heat the medium sharply, whilst endothermic processes cool it).
### 4. Particle physics (Visualisation)
* **Gases:** For each gas particle, the root-mean-square velocity is calculated using the formula:  
$$v_{rms} = \sqrt{\frac{3RT}{M}}$$  
Heavy molecules move more slowly, while light ones (e.g. hydrogen) move considerably faster. When heated, the chaos and velocity increase visibly.
* **Liquids:** Liquid particles are sorted by density ($\rho$). Denser substances (e.g. mercury) physically settle to the very bottom, pushing lighter organic compounds (e.g. hexane) upwards, forming clear phase boundaries.
### 5. Interface
The application interface is built entirely using the `eframe` and `egui` libraries. The key feature of the architecture is **Immediate Mode UI**. This means that the interface is not stored in memory as a widget tree, but is completely redrawn from scratch every frame based on the application’s current state.  
### Modular window system
To avoid overwhelming the user with information, the workspace is divided into **7 functional windows** (`egui::Window`), each of which is responsible for its own isolated context:
1. **Reactor control:** Sliders for adjusting the physical parameters of the environment and a button for manually igniting the mixture.
2. **Warning lights:** A warning panel that dynamically changes colour when parameters exceed safe limits.
3. **Component status:** Current thermodynamic parameters and the exact molar composition of the contents (gas/liquid).
4. **Monitoring:** Implemented using `egui_plot`. Displays real-time graphs of pressure and temperature changes, with a history limit of 400 data points to optimise memory usage.
5. **Gateway:** Interface for adding new substances to the system and fully purging the chamber.
6. **Chamber (Viewport):** Particle visualisation window.
7. **Substance constants:** Interactive database reference.
### Custom rendering and graphical solutions
* **Particle rendering via Painter:** The ‘Camera’ window uses the low-level `ui.allocate_painter` tool. Instead of heavy objects, the simulator draws 2D primitives (`painter.circle_filled`) directly onto the canvas. The colour of the particles is generated dynamically based on a hash of the substance’s name, ensuring a unique shade for each gas.
* **Status LEDs:** A custom function `draw_lamp` has been written for the indicator lights. It allocates a precise area on the screen and draws a coloured indicator, mimicking a metal control panel.
* **Optimised reference guide:** The constants window uses a combination of `egui::ScrollArea` and `ui.collapsing`. Data is rendered only when a node is expanded by the user.

# Features
The application simulates the behaviour of real gases and liquids in a closed or ventilated space, based on the fundamental laws of thermodynamics and physical chemistry. The project allows users to observe phase transitions, pressure increases and the progression of multi-component chemical reactions (from hydrogen combustion to methane oxidation).
* **Support for 30 substances:** from noble gases to heavy hydrocarbons, alcohols and mercury, with real physical constants.
* **Realistic pressure calculation:** the simulation uses the Van der Waals equation of state for real gases, rather than idealised formulas.
* **Dynamic visualisation:** gases behave as chaotic particles with velocities dependent on temperature, whilst liquids separate into layers depending on their density.
* **Interactive control:** the ability to change the chamber volume, adjust the thermal conductivity of the walls, inject new substances, open the emergency valve and initiate reactions with a spark.
