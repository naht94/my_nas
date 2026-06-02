<script>
    let username = '';
    let password = '';
    let confirmPassword = '';
    let errorMessage = '';
    let successMessage = '';

    async function handleRegister() {
        errorMessage = '';
        successMessage = '';

        if (!username || !password) {
            errorMessage = '아이디와 비밀번호를 모두 입력해주세요.';
            return;
        }
        if (password !== confirmPassword) {
            errorMessage = '비밀번호가 일치하지 않습니다.';
            return;
        }

        try {
            const response = await fetch('http://localhost:3000/api/users/register', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify({ username, password }) 
            });

            if (response.ok) {
                const data = await response.json();
                successMessage = data.message;
                username = '';
                password = '';
                confirmPassword = '';
            } else {
                errorMessage = '회원가입에 실패했습니다. 아이디가 중복되었을 수 있습니다.';
            }
        } catch (error) {
            console.error('API Error:', error);
            errorMessage = '서버와 통신할 수 없습니다. 백엔드가 켜져 있는지 확인해 주세요.';
        }
    }
</script>

<main class="container">
    <h2>NAS 회원가입</h2>

    <form on:submit|preventDefault={handleRegister}>
        <div class="input-group">
            <label for="username">아이디</label>
            <input type="text" id="username" bind:value={username} placeholder="아이디를 입력하세요" />
        </div>

        <div class="input-group">
            <label for="password">비밀번호</label>
            <input type="password" id="password" bind:value={password} placeholder="비밀번호" />
        </div>

        <div class="input-group">
            <label for="confirmPassword">비밀번호 확인</label>
            <input type="password" id="confirmPassword" bind:value={confirmPassword} placeholder="비밀번호 확인" />
        </div>

        {#if errorMessage}
            <p class="error">{errorMessage}</p>
        {/if}
        {#if successMessage}
            <p class="success">{successMessage}</p>
        {/if}

        <button type="submit">가입하기</button>
    </form>
</main>

<style>
    .container {
        max-width: 400px;
        margin: 50px auto;
        padding: 20px;
        border: 1px solid #ddd;
        border-radius: 8px;
    }
    .input-group {
        margin-bottom: 15px;
    }
    .input-group label {
        display: block;
        margin-bottom: 5px;
        font-weight: bold;
    }
    .input-group input {
        width: 100%;
        padding: 8px;
        box-sizing: border-box;
    }
    button {
        width: 100%;
        padding: 10px;
        background-color: #007BFF;
        color: white;
        border: none;
        border-radius: 4px;
        cursor: pointer;
    }
    button:hover { background-color: #0056b3; }
    .error { color: red; font-size: 0.9em; }
    .success { color: green; font-size: 0.9em; }
</style>
